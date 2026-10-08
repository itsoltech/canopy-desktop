use super::{components::*, theme as t};
use crate::app_state::{AppState, ChangesState};
use canopy_desktop::git::changes::{Edit, FileChange};
use canopy_desktop::motion::{self, Transition, presets};
use gpui_kit::{
    base::Disableable,
    component::{
        IconName,
        input::{InputEvent, InputState, TextareaState},
    },
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
        let events = vec![
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
                self.rows.push(ChangeRow::Header(
                    staged,
                    data.files.iter().filter(|f| f.staged == staged).count(),
                ));
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
                        button(
                            if staged { "unstage-all" } else { "stage-all" },
                            if staged { "Unstage all" } else { "Stage all" },
                        )
                        .px(px(4.))
                        .border_0()
                        .bg(rgba(0))
                        .text_size(px(11.))
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
            Some(ChangeRow::File(i)) => self
                .state
                .read(cx)
                .data
                .as_ref()
                .and_then(|d| d.files.get(*i))
                .map(|f| self.file_row(f, disabled, cx).into_any_element())
                .unwrap_or_else(|| div().into_any_element()),
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
    fn file_row(&self, file: &FileChange, disabled: bool, cx: &Context<Self>) -> Stateful<Div> {
        let key = format!("{}:{}", file.staged, file.path.display());
        let hover = key.clone();
        let diff = file.clone();
        let edit = file.clone();
        let discard = file.clone();
        let path = self.state.read(cx).path.clone();
        let edit_path = path.clone();
        let diff_path = path.clone();
        let label = file.path.display().to_string();
        git::change_file_row(SharedString::from(key.clone()), &file.kind)
            .on_hover(cx.listener(move |this, over, _, cx| {
                if *over {
                    this.hovered = Some(hover.clone());
                } else if this.hovered.as_ref() == Some(&hover) {
                    this.hovered = None;
                }
                cx.notify();
            }))
            .child(
                list_button(
                    SharedString::from(format!("diff-{key}")),
                    label,
                    self.hovered.as_ref() == Some(&key),
                    cx,
                )
                .px_0()
                .disabled(self.inactive(cx))
                .flex_1()
                .min_w_0()
                .tooltip(
                    file.old_path
                        .as_ref()
                        .map(|old| format!("{} → {}", old.display(), file.path.display()))
                        .unwrap_or_else(|| file.path.display().to_string()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.state.read(cx).path == diff_path {
                        this.state.update(cx, |s, cx| s.open_diff(diff.clone(), cx));
                    }
                })),
            )
            .child(
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
                })),
            )
            .children((!file.staged).then(|| {
                git::change_action(
                    SharedString::from(format!("discard-{key}")),
                    IconName::Close,
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
        let signing = data
            .as_ref()
            .map(|d| {
                if d.signing.enabled {
                    format!("Signed commit · {}", d.signing.format)
                } else {
                    "Unsigned commit".into()
                }
            })
            .unwrap_or_default();

        let tabs =
            row()
                .gap(px(4.))
                .children([("Files", false), ("History", true)].into_iter().map(
                    |(label, history)| {
                        selection_button(label, label, self.show_history == history)
                            .border_0()
                            .flex_1()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.show_history == history {
                                    return;
                                }
                                this.show_history = history;
                                let now = Instant::now();
                                this.content = Transition::new(0., now);
                                this.content.retarget(
                                    1.,
                                    presets::STATE_CHANGE,
                                    now,
                                    motion::policy(cx),
                                );
                                if history {
                                    let path = this.state.read(cx).path.clone();
                                    this.history.update(cx, |view, cx| {
                                        view.open(path, cx);
                                    });
                                }
                                cx.notify();
                            }))
                    },
                ));
        let transfer_footer = column()
            .flex_shrink_0()
            .py(px(t::SPACING_UNIT * 3.))
            .gap(px(t::SPACING_UNIT * 2.))
            .border_t_1()
            .border_color(t::border())
            .child(git_network::transfer_controls(|operation| {
                let feedback = match operation {
                    canopy_desktop::git::network::Operation::Pull => &self.pull_loading,
                    canopy_desktop::git::network::Operation::Push => &self.push_loading,
                };
                git_network::transfer_button(operation, feedback)
                    .disabled((disabled || state.path.is_none()) && !feedback.active())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.error = this
                            .state
                            .update(cx, |state, cx| state.network(operation, None, cx))
                            .err()
                            .map(|e| e.to_string());
                        cx.notify();
                    }))
            }))
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
                .pt(px(12.))
                .gap(px(8.))
                .child(tabs)
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .opacity(opacity)
                        .child(self.history.clone()),
                )
                .child(transfer_footer)
                .into_any_element();
        }
        let files_content = column()
            .flex_1()
            .min_h_0()
            .gap(px(8.))
            .child(git::inspector_toolbar(
                format!(
                    "{} change{}",
                    files.len(),
                    if files.len() == 1 { "" } else { "s" }
                ),
                icon_button("refresh-changes", IconName::RotateCw, "Refresh changes")
                    .disabled(disabled)
                    .on_click(
                        cx.listener(|this, _, _, cx| this.state.update(cx, |s, cx| s.refresh(cx))),
                    ),
            ))
            .child(input(&self.filter).w_full().disabled(self.inactive(cx)))
            .child(
                row()
                    .gap(px(4.))
                    .children(["All", "A", "M", "D", "R"].into_iter().map(|kind| {
                        selection_button(
                            kind,
                            kind,
                            self.kind.as_deref() == Some(kind)
                                || (kind == "All" && self.kind.is_none()),
                        )
                        .px(px(8.))
                        .border_0()
                        .h(px(24.))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.kind = (kind != "All").then(|| kind.into());
                            this.rebuild(cx);
                            cx.notify();
                        }))
                    })),
            )
            .children(data.is_none().then(|| {
                div().text_size(px(11.)).text_color(t::muted()).child(
                    if state.path.as_ref().is_some_and(|p| {
                        cx.global::<AppState>().git.read(cx).repository(p).is_some()
                    }) {
                        "Loading changes…"
                    } else {
                        "No Git repository selected."
                    },
                )
            }))
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
                .min_h_0(),
            )
            .child(
                git::commit_form()
                    .child(
                        textarea(&self.message)
                            .h(px(72.))
                            .w_full()
                            .disabled(disabled),
                    )
                    .child(div().text_size(px(11.)).text_color(t::muted()).child(
                        if state.committing {
                            state
                                .commit_phase
                                .clone()
                                .unwrap_or_else(|| "Committing…".into())
                        } else {
                            signing
                        },
                    ))
                    .child(
                        primary_loading_button(
                            "commit-staged",
                            "Commit staged",
                            &self.commit_loading,
                        )
                        .w_full()
                        .disabled(
                            (disabled
                                || staged == 0
                                || self.message.read(cx).value().trim().is_empty())
                                && !self.commit_loading.active(),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
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
                        })),
                    )
                    .children(state.committing.then(|| {
                        button("cancel-commit", "Cancel commit")
                            .on_click(cx.listener(|this, _, _, cx| this.state.read(cx).cancel()))
                    })),
            );
        column()
            .size_full()
            .pt(px(12.))
            .gap(px(8.))
            .child(tabs)
            .child(files_content.opacity(opacity))
            .child(transfer_footer)
            .into_any_element()
    }
}
