//! "Open diff" of a chat turn: every file the agent changed with Edit/Write in that
//! turn, as one scrolling diff in its own pane.
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::tools::{FileChange, TurnDiffTarget};
use canopy_desktop::git::changes::DiffLine;
use gpui_kit::*;

enum Line {
    File(usize),
    Diff(DiffLine),
}

pub struct TurnDiffView {
    files: Vec<FileChange>,
    lines: Vec<Line>,
    longest: usize,
    cwd: Option<std::path::PathBuf>,
    scroll: UniformListScrollHandle,
    /// The chat session that made the changes is gone (closed, restarted, app restart).
    unavailable: bool,
}

impl TurnDiffView {
    /// `files` is `None` when the chat no longer has this turn.
    pub fn new(
        target: &TurnDiffTarget,
        files: Option<Vec<FileChange>>,
        cwd: Option<std::path::PathBuf>,
    ) -> Self {
        let unavailable = files.is_none();
        let files = files.unwrap_or_default();
        let mut lines = vec![];
        let mut first_row = None;
        for (index, file) in files.iter().enumerate() {
            if target.file.as_ref() == Some(&file.path) {
                first_row = Some(lines.len());
            }
            lines.push(Line::File(index));
            lines.extend(file.lines.iter().cloned().map(Line::Diff));
        }
        // The list measures this row to size its horizontal extent.
        let longest = lines
            .iter()
            .enumerate()
            .max_by_key(|(_, line)| match line {
                Line::Diff(diff) => diff.text.chars().count(),
                Line::File(_) => 0,
            })
            .map_or(0, |(index, _)| index);
        let scroll = UniformListScrollHandle::new();
        if let Some(row) = first_row {
            scroll.scroll_to_item(row, ScrollStrategy::Top);
        }
        Self {
            files,
            lines,
            longest,
            cwd,
            scroll,
            unavailable,
        }
    }
}

impl Render for TurnDiffView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.unavailable {
            return column()
                .size_full()
                .items_center()
                .justify_center()
                .bg(t::bg())
                .text_size(px(13.))
                .text_color(t::muted())
                .child("This turn's changes are no longer available.")
                .into_any_element();
        }
        let added: usize = self.files.iter().map(|f| f.added).sum();
        let removed: usize = self.files.iter().map(|f| f.removed).sum();
        column()
            .size_full()
            .bg(t::bg())
            .child(
                row()
                    .h(px(36.))
                    .flex_shrink_0()
                    .px(px(12.))
                    .gap(px(8.))
                    .border_b_1()
                    .border_color(t::border())
                    .text_size(px(13.))
                    .child(div().text_color(t::text()).child(format!(
                        "{} changed {}",
                        self.files.len(),
                        if self.files.len() == 1 {
                            "file"
                        } else {
                            "files"
                        }
                    )))
                    .child(div().text_color(t::green()).child(format!("+{added}")))
                    .child(div().text_color(t::red()).child(format!("−{removed}"))),
            )
            .child(
                uniform_list(
                    "turn-diff-lines",
                    self.lines.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, _| {
                        range
                            .filter_map(|index| this.lines.get(index))
                            .map(|line| match line {
                                Line::Diff(diff) => git::diff_line(diff).into_any_element(),
                                Line::File(index) => {
                                    file_header(&this.files[*index], this.cwd.as_deref())
                                        .into_any_element()
                                }
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.scroll)
                .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                .with_width_from_item(Some(self.longest))
                .flex_1()
                .min_h_0(),
            )
            .into_any_element()
    }
}

/// The file's path relative to the chat folder, with its counts.
pub(super) fn display_path(path: &std::path::Path, cwd: Option<&std::path::Path>) -> String {
    cwd.and_then(|cwd| path.strip_prefix(cwd).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}

fn file_header(file: &FileChange, cwd: Option<&std::path::Path>) -> Div {
    // Same height as a diff line: the list gives every row one measured height.
    row()
        .h(px(24.))
        .px(px(12.))
        .gap(px(8.))
        .bg(t::elevated())
        .border_y_1()
        .border_color(t::border())
        .text_size(px(12.))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_color(t::text())
                .child(display_path(&file.path, cwd)),
        )
        .children(
            file.created
                .then(|| div().text_color(t::muted()).child("new")),
        )
        .child(
            div()
                .text_color(t::green())
                .child(format!("+{}", file.added)),
        )
        .child(
            div()
                .text_color(t::red())
                .child(format!("−{}", file.removed)),
        )
}
