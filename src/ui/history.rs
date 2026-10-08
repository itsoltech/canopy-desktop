use super::{
    components::{history as history_ui, *},
    theme as t,
};
use crate::app_state::AppState;
use canopy_desktop::git::{
    history::{CommitEntry, HistorySnapshot},
    history_graph::{GraphRow, HistoryGraph},
};
use canopy_desktop::motion::{self, BatchReveal, Presence, presets};
use gpui_kit::{base::Disableable, component::IconName, *};
use std::path::PathBuf;
use std::time::Instant;

pub struct HistoryView {
    path: Option<PathBuf>,
    snapshot: Option<std::sync::Arc<HistorySnapshot>>,
    entries: Vec<CommitEntry>,
    selected: Option<usize>,
    details: Presence,
    graph: HistoryGraph,
    graph_rows: Vec<GraphRow>,
    graph_width: usize,
    loading: bool,
    refresh_loading: ButtonLoading,
    more: bool,
    error: Option<String>,
    generation: u64,
    task: Option<Task<()>>,
    reveal: BatchReveal,
}
impl HistoryView {
    pub fn new() -> Self {
        Self {
            path: None,
            snapshot: None,
            entries: vec![],
            selected: None,
            details: Presence::new(false, presets::PANEL, Instant::now()),
            graph: HistoryGraph::default(),
            graph_rows: vec![],
            graph_width: 1,
            loading: false,
            refresh_loading: ButtonLoading::default(),
            more: false,
            error: None,
            generation: 0,
            task: None,
            reveal: BatchReveal::default(),
        }
    }
    pub fn open(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        self.path = path;
        self.refresh(cx);
    }
    pub fn set_path(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.path != path {
            self.path = path;
            self.refresh(cx);
        }
    }
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        self.task = None;
        self.snapshot = None;
        self.entries.clear();
        self.reveal.reset();
        self.selected = None;
        self.details = Presence::new(false, presets::PANEL, Instant::now());
        self.graph = HistoryGraph::default();
        self.graph_rows.clear();
        self.graph_width = 1;
        self.error = None;
        self.more = false;
        self.loading = false;
        self.refresh_loading.set(false, cx);
        self.load(cx);
        cx.notify();
    }
    fn load(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let Some(path) = self.path.clone() else {
            return;
        };
        let Some(client) = cx.global::<AppState>().changes.read(cx).client.clone() else {
            return;
        };
        let head = self.snapshot.clone();
        let offset = self.entries.len();
        let generation = self.generation;
        self.loading = true;
        self.refresh_loading.set(offset == 0, cx);
        self.error = None;
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = client.history(path, head, offset).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                this.refresh_loading.set(false, cx);
                match result {
                    Ok(page) => {
                        this.snapshot = page.snapshot;
                        this.more = page.more;
                        for commit in &page.commits {
                            let row = this.graph.append(&commit.id, &commit.parents);
                            this.graph_width = this.graph_width.max(row.width);
                            this.graph_rows.push(row);
                        }
                        let now = Instant::now();
                        this.reveal.begin(
                            this.entries.len(),
                            page.commits.len(),
                            now,
                            motion::policy(cx),
                        );
                        this.entries.extend(page.commits);
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
    }
}
impl Render for HistoryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let progress = self.details.progress(now);
        motion::request_frame(
            window,
            self.details.is_animating(now) || self.reveal.is_animating(now),
        );
        let initial_loading = self.loading && self.entries.is_empty();
        column()
            .size_full()
            .pb(px(t::SPACING_UNIT * 2.))
            .gap(px(8.))
            .child(git::inspector_toolbar(
                "Current branch",
                loading_icon_button(
                    "refresh-history",
                    IconName::RotateCw,
                    "Refresh history",
                    Some(&self.refresh_loading),
                )
                .disabled(self.loading && !self.refresh_loading.active())
                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            ))
            .child(git_tracking::tracking_summary(self.snapshot.as_deref()))
            .children(
                self.error
                    .clone()
                    .map(|error| div().text_size(px(11.)).text_color(t::red()).child(error)),
            )
            .children(
                (self.entries.is_empty() && self.error.is_none() && !self.loading).then(|| {
                    div()
                        .py(px(16.))
                        .text_color(t::muted())
                        .child(if self.path.is_none() {
                            "No Git repository selected."
                        } else {
                            "No commits to show."
                        })
                }),
            )
            .child(
                uniform_list(
                    "commit-history",
                    if initial_loading {
                        8
                    } else {
                        self.entries.len()
                            + if self.loading {
                                3
                            } else {
                                usize::from(self.more || self.error.is_some())
                            }
                    },
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| {
                                if this.loading && this.entries.is_empty() {
                                    return history_ui::commit_skeleton(index).into_any_element();
                                }
                                if index >= this.entries.len() {
                                    if this.loading {
                                        return history_ui::commit_skeleton(
                                            index - this.entries.len(),
                                        )
                                        .into_any_element();
                                    }
                                    let opacity = this
                                        .entries
                                        .len()
                                        .checked_sub(1)
                                        .map(|last| this.reveal.opacity(last, Instant::now()))
                                        .unwrap_or(1.);
                                    return row()
                                        .w_full()
                                        .h(px(32.))
                                        .py(px(2.))
                                        .opacity(opacity)
                                        .child(
                                            button(
                                                "more-history",
                                                if this.error.is_some() {
                                                    "Try again"
                                                } else {
                                                    "Load more"
                                                },
                                            )
                                            .w_full()
                                            .on_click(cx.listener(|this, _, _, cx| this.load(cx))),
                                        )
                                        .into_any_element();
                                }
                                let opacity = this.reveal.opacity(index, Instant::now());
                                let commit = &this.entries[index];
                                let selected = this.selected == Some(index);
                                history_ui::commit_row(
                                    commit,
                                    &this.graph_rows[index],
                                    this.graph_width,
                                    selected,
                                )
                                .opacity(opacity)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.selected = Some(index);
                                    this.details
                                        .set_open(true, Instant::now(), motion::policy(cx));
                                    cx.notify();
                                }))
                                .into_any_element()
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .w_full()
                .flex_1()
                .min_h_0(),
            )
            .children(
                self.selected
                    .filter(|_| progress > 0.)
                    .and_then(|i| self.entries.get(i))
                    .map(|commit| {
                        history_ui::commit_details(
                            commit,
                            progress,
                            icon_button(
                                "close-commit-details",
                                IconName::Close,
                                "Close commit details",
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.details
                                    .set_open(false, Instant::now(), motion::policy(cx));
                                cx.notify();
                            })),
                        )
                    }),
            )
    }
}
