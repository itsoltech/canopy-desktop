//! Tool cards (diffs, previews, command output), thinking rows and the summary of
//! files a turn changed.
use super::{
    rows::{Row, ToolCard},
    turn_diff::display_path,
    view::ChatView,
};
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::tools::{FileChange, ToolView, TreeRow};
use canopy_desktop::git::changes::DiffLine;
use gpui_kit::component::{
    IconName, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    shimmer::ShimmerText,
};
use gpui_kit::{prelude::FluentBuilder, *};
use std::time::Instant;

/// Lines of a diff or preview shown before "Show more".
const PREVIEW_LINES: usize = 12;
const OUTPUT_CHARS: usize = 20_000;

/// The verb and detail a card shows instead of the raw tool name.
pub(super) fn header(card: &ToolCard) -> Option<(&'static str, Option<AnyElement>)> {
    Some(match &card.view {
        ToolView::Change(change) => (
            if change.created { "Created" } else { "Edited" },
            Some(counts(change.added, change.removed).into_any_element()),
        ),
        ToolView::Read {
            start,
            shown,
            total,
            ..
        } => (
            "Read",
            (*shown > 0).then(|| {
                let end = start + shown.saturating_sub(1);
                div()
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(t::faint())
                    .child(if *start <= 1 && end >= *total {
                        format!("{total} lines")
                    } else {
                        format!("lines {start}–{end} of {total}")
                    })
                    .into_any_element()
            }),
        ),
        _ => return None,
    })
}

pub(super) fn counts(added: usize, removed: usize) -> Div {
    row()
        .flex_shrink_0()
        .gap(px(6.))
        .text_size(px(12.))
        .font_family(t::MONO)
        .child(div().text_color(t::green()).child(format!("+{added}")))
        .child(div().text_color(t::red()).child(format!("−{removed}")))
}

/// Whether the card has anything to unfold.
pub(super) fn has_body(card: &ToolCard) -> bool {
    match &card.view {
        ToolView::Change(change) => !change.lines.is_empty(),
        ToolView::Read { content, .. } => !content.trim().is_empty(),
        ToolView::Shell { stdout, stderr, .. } => {
            !stdout.trim().is_empty() || !stderr.trim().is_empty()
        }
        ToolView::Web { text, .. } => !text.trim().is_empty(),
        ToolView::Generic { output, .. } => !output.trim().is_empty(),
    }
}

/// The unfolded content of a card.
pub(super) fn body(view: &Entity<ChatView>, entry: &Row, card: &ToolCard) -> Div {
    let key = entry.key;
    let expanded = entry.extra.expanded;
    let inner = match &card.view {
        ToolView::Change(change) => {
            let total = change.lines.len();
            let shown = if expanded {
                total
            } else {
                total.min(PREVIEW_LINES)
            };
            let longest = change.lines[..shown]
                .iter()
                .map(|line| canopy_desktop::markdown::columns(&line.text))
                .max()
                .unwrap_or(0);
            // One number column, as wide as the largest number in the card.
            let digits = change
                .lines
                .iter()
                .filter_map(|line| line.new.or(line.old))
                .max()
                .unwrap_or(1)
                .to_string()
                .len();
            let gutter = 7.3 * digits as f32 + 20.;
            // As wide as the longest line, so the +/- backgrounds follow the text when
            // it scrolls sideways.
            let width = px(gutter + 16. + 7.3 * longest as f32 + 16.);
            framed(
                div()
                    .id(("chat-diff", key))
                    .w_full()
                    .overflow_x_scroll()
                    .child(
                        column().w(width).min_w_full().children(
                            change.lines[..shown]
                                .iter()
                                .map(|line| diff_line(line, gutter)),
                        ),
                    ),
            )
            .children(more(view, key, expanded, total, shown))
            .into_any_element()
        }
        ToolView::Read { start, content, .. } => {
            let total = content.lines().count();
            let shown = if expanded {
                total
            } else {
                total.min(PREVIEW_LINES)
            };
            // A prefix of the original text keeps the highlight offsets valid (CRLF too).
            let end = content
                .match_indices('\n')
                .nth(shown.saturating_sub(1))
                .map_or(content.len(), |(at, _)| at);
            let visible = &content[..end];
            // CRLF files are shown without colors: dropping `\r` would shift every offset.
            let crlf = visible.contains('\r');
            let highlights: Vec<_> = card
                .highlights
                .iter()
                .filter(|(range, _)| {
                    !crlf
                        && range.end <= visible.len()
                        && visible.is_char_boundary(range.start)
                        && visible.is_char_boundary(range.end)
                })
                .cloned()
                .collect();
            let visible = if crlf {
                visible.replace('\r', "")
            } else {
                visible.to_owned()
            };
            framed(
                row()
                    .w_full()
                    .items_start()
                    .py(px(8.))
                    .font_family(t::MONO)
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .child(
                        column()
                            .flex_shrink_0()
                            .pl(px(10.))
                            .pr(px(10.))
                            .items_end()
                            .text_color(t::faint())
                            .children((0..shown).map(|i| {
                                div().h(px(18.)).child((*start as usize + i).to_string())
                            })),
                    )
                    .child(
                        div()
                            .id(("chat-read", key))
                            .flex_1()
                            .min_w_0()
                            .overflow_x_scroll()
                            .pr(px(12.))
                            .whitespace_nowrap()
                            .text_color(t::text())
                            .child(StyledText::new(visible).with_highlights(highlights)),
                    ),
            )
            .children(more(view, key, expanded, total, shown))
            .into_any_element()
        }
        ToolView::Shell { stdout, stderr, .. } => output_box(key)
            .children(
                (!stdout.trim().is_empty())
                    .then(|| div().text_color(t::secondary()).child(clip(stdout))),
            )
            .children((!stderr.trim().is_empty()).then(|| {
                column()
                    .when(!stdout.trim().is_empty(), |c| {
                        c.mt(px(8.))
                            .pt(px(6.))
                            .border_t_1()
                            .border_color(t::border())
                    })
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(t::faint())
                            .child("stderr"),
                    )
                    .child(div().text_color(t::red()).child(clip(stderr)))
            }))
            .nested_scroll()
            .into_any_element(),
        ToolView::Web { text, .. } | ToolView::Generic { output: text, .. } => output_box(key)
            .text_color(t::secondary())
            .child(clip(text))
            .nested_scroll()
            .into_any_element(),
    };
    // Bodies start at the text column, like the prose around them.
    div().mt(px(4.)).child(inner)
}

/// A compact diff row for cards: one line number (the old one for removals), the
/// sign and the text, on the change's background.
pub(super) fn diff_line(line: &DiffLine, gutter: f32) -> Div {
    let changed = matches!(line.kind, '+' | '-');
    let color = if line.kind == '+' {
        t::green()
    } else {
        t::red()
    };
    let number = match line.kind {
        '-' => line.old,
        _ => line.new.or(line.old),
    };
    row()
        .w_full()
        .min_w_full()
        .h(px(22.))
        .font_family(t::MONO)
        .text_size(px(12.))
        .text_color(if line.kind == 'H' {
            t::muted()
        } else {
            t::text()
        })
        .bg(t::diff_line_background(line.kind))
        .border_l_2()
        .border_color(if changed {
            color.opacity(0.7)
        } else {
            rgba(0).into()
        })
        .child(
            div()
                .w(px(gutter))
                .flex_shrink_0()
                .pr(px(10.))
                .text_right()
                .text_color(if changed {
                    color.opacity(0.65)
                } else {
                    t::faint()
                })
                .child(number.map(|n| n.to_string()).unwrap_or_default()),
        )
        .child(
            div()
                .w(px(16.))
                .flex_shrink_0()
                .text_color(if changed { color } else { t::faint() })
                .child(match line.kind {
                    '+' | '-' => line.kind.to_string(),
                    _ => String::new(),
                }),
        )
        .child(
            div()
                .flex_shrink_0()
                .whitespace_nowrap()
                .child(line.text.replace('\t', "    ")),
        )
}

fn framed(content: impl IntoElement) -> Div {
    column()
        .w_full()
        .rounded(px(6.))
        .border_1()
        .border_color(t::border())
        .bg(t::elevated())
        .overflow_hidden()
        .child(content)
}

fn output_box(key: u64) -> Stateful<Div> {
    div()
        .id(("chat-tool-output-scroll", key))
        .max_h(px(240.))
        .overflow_y_scroll()
        .p(px(10.))
        .rounded(px(6.))
        .bg(t::elevated())
        .font_family(t::MONO)
        .text_size(px(12.))
}

fn clip(text: &str) -> String {
    const LINES: usize = 400;
    let mut clipped: String = text.chars().take(OUTPUT_CHARS).collect();
    if let Some((at, _)) = clipped.match_indices('\n').nth(LINES) {
        clipped.truncate(at);
    }
    if clipped.len() < text.len() {
        clipped.push_str("\n… output truncated");
    }
    clipped
}

/// "Show N more lines" / "Show less" under a long diff or preview.
fn more(
    view: &Entity<ChatView>,
    key: u64,
    expanded: bool,
    total: usize,
    shown: usize,
) -> Option<Button> {
    (total > PREVIEW_LINES).then(|| {
        let view = view.downgrade();
        Button::new(("chat-more", key))
            .ghost()
            .xsmall()
            .compact()
            .w_full()
            .h(px(26.))
            .rounded(px(0.))
            .border_t_1()
            .border_color(t::border())
            .text_size(px(12.))
            .text_color(t::muted())
            .label(if expanded {
                "Show less".to_owned()
            } else {
                format!("Show {} more lines", total - shown)
            })
            .on_click(move |_, _, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.shown_rows_mut().toggle_expanded(key, cx);
                    cx.notify();
                });
            })
    })
}

/// A thinking summary: live while it streams, then folded to "Thought for Ns".
/// "2m 41s", "5s" or "a moment".
pub(super) fn elapsed(seconds: f32) -> String {
    match seconds {
        s if s >= 60. => {
            let s = s.round() as u64;
            format!("{}m {}s", s / 60, s % 60)
        }
        s if s >= 1. => format!("{}s", s.round() as u64),
        _ => "a moment".to_owned(),
    }
}

pub(super) fn thinking_row(
    view: &Entity<ChatView>,
    entry: &Row,
    text: &str,
    streaming: bool,
    cx: &App,
) -> Div {
    let key = entry.key;
    let now = Instant::now();
    let label: AnyElement = if streaming {
        ShimmerText::new("Thinking…")
            .id(("chat-thought", key))
            .text_size(px(13.))
            .text_color(t::muted())
            .highlight_color(t::text())
            .into_any_element()
    } else {
        let seconds = entry
            .extra
            .thought
            .and_then(|(started, ended)| Some(ended?.duration_since(started?).as_secs_f32()));
        div()
            .text_size(px(13.))
            .text_color(t::muted())
            .child(match seconds {
                Some(s) => format!("Thought for {}", elapsed(s)),
                None => "Thought".to_owned(),
            })
            .into_any_element()
    };
    let has_text = !text.trim().is_empty();
    let line = row()
        .gap(px(6.))
        .child(custom_icon("brain").size(px(14.)).text_color(t::muted()))
        .child(label)
        .children(
            entry
                .output
                .as_ref()
                .filter(|_| has_text)
                .map(|d| d.chevron(now).text_color(t::faint())),
        );
    // A plain left-aligned target (a Button would center its content); the text
    // brightens on hover, like other headers.
    let _ = cx;
    let header: AnyElement = if has_text {
        let view = view.downgrade();
        line.id(("chat-thinking-toggle", key))
            .keyboard("Show or hide the thinking")
            .h(px(24.))
            .cursor_pointer()
            .hover(|style| style.text_color(t::secondary()))
            .on_click(move |_, _, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.shown_rows_mut().toggle_output(key, cx);
                    cx.notify();
                });
            })
            .into_any_element()
    } else {
        line.h(px(24.)).into_any_element()
    };
    column()
        .pt(px(10.))
        .child(header)
        .children(
            entry
                .output
                .as_ref()
                .filter(|_| has_text)
                .map(|disclosure| {
                    disclosure.measured_body(
                        ("chat-thinking-body", key),
                        // Capped while it streams; once done the list scrolls through it,
                        // so search can reveal any line of it.
                        div()
                            .id(("chat-thinking-scroll", key))
                            .when(streaming, |body| body.max_h(px(240.)).overflow_y_scroll())
                            .pl(px(22.))
                            .pt(px(2.))
                            .text_size(px(13.))
                            .line_height(px(20.))
                            .text_color(t::muted())
                            .map(|body| match &entry.markdown {
                                Some(markdown) => body.child(markdown.clone()),
                                None => body.child(text.trim().to_owned()),
                            })
                            .nested_scroll(),
                        now,
                    )
                }),
        )
}

/// "N changed files +A −D", a folder tree of the files and "Open diff".
pub(super) fn changes_card(
    view: &Entity<ChatView>,
    this: &ChatView,
    entry: &Row,
    turn: u64,
    files: &[FileChange],
    epoch: Option<u64>,
    cx: &App,
) -> Div {
    let key = entry.key;
    let cwd = this.pane.metadata.cwd.clone();
    let added: usize = files.iter().map(|f| f.added).sum();
    let removed: usize = files.iter().map(|f| f.removed).sum();
    let open = |file: Option<std::path::PathBuf>| {
        let view = view.downgrade();
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            let _ = view.update(cx, |this, cx| {
                this.open_turn_diff(epoch, turn, file.clone(), cx)
            });
        }
    };
    let open_diff = Button::new(("chat-changes-open", key))
        .ghost()
        .xsmall()
        .compact()
        .h(px(24.))
        .px(px(8.))
        .rounded(px(6.))
        .text_size(px(12.))
        .icon(icon(IconName::FileText))
        .label("Open diff")
        .tooltip("Open this turn's changes")
        .on_click(open(None));
    let tree = canopy_desktop::chat::tools::change_tree(
        files
            .iter()
            .map(|file| (display_path(&file.path, cwd.as_deref()), file)),
        &entry.extra.collapsed,
    );
    let rows: Vec<AnyElement> = tree
        .into_iter()
        .map(|node| match node {
            TreeRow::Folder {
                path,
                name,
                depth,
                added,
                removed,
            } => {
                let folded = entry.extra.collapsed.contains(&path);
                let view = view.downgrade();
                tree_row(
                    ElementId::Name(format!("chat-changes-{key}-dir-{path}").into()),
                    depth,
                    cx,
                )
                .accessibility_label(format!(
                    "{} folder {name}",
                    if folded { "Expand" } else { "Collapse" }
                ))
                .child(
                    row()
                        .w_full()
                        .gap(px(6.))
                        .child(
                            icon(if folded {
                                IconName::ChevronRight
                            } else {
                                IconName::ChevronDown
                            })
                            .size(px(10.))
                            .text_color(t::faint()),
                        )
                        .child(icon(IconName::Folder).text_color(t::muted()))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(t::secondary())
                                .child(name),
                        )
                        .child(counts(added, removed)),
                )
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.shown_rows_mut().toggle_folder(key, path.clone(), cx);
                        cx.notify();
                    });
                })
                .into_any_element()
            }
            TreeRow::File {
                change: file,
                name,
                depth,
            } => tree_row(
                ElementId::Name(format!("chat-changes-{key}-file-{}", file.path.display()).into()),
                depth,
                cx,
            )
            .accessibility_label(format!("Open the diff of {name}"))
            .child(
                row()
                    .w_full()
                    .gap(px(6.))
                    .child(match crate::ui::file_icons::for_path(&file.path) {
                        Some(asset) => img(asset).size(px(14.)).flex_shrink_0().into_any_element(),
                        None => icon(IconName::File)
                            .text_color(t::muted())
                            .into_any_element(),
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(t::text())
                            .child(name),
                    )
                    .children(
                        file.created
                            .then(|| div().text_size(px(11.)).text_color(t::faint()).child("new")),
                    )
                    .child(counts(file.added, file.removed)),
            )
            .on_click(open(Some(file.path.clone())))
            .into_any_element(),
        })
        .collect();
    column()
        .mt(px(12.))
        .rounded(px(10.))
        .border_1()
        .border_color(t::border())
        .bg(t::elevated())
        .overflow_hidden()
        .child(
            row()
                .h(px(40.))
                .px(px(12.))
                .gap(px(8.))
                .border_b_1()
                .border_color(t::border())
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(t::text())
                        .child(format!(
                            "{} changed {}",
                            files.len(),
                            if files.len() == 1 { "file" } else { "files" }
                        )),
                )
                .child(counts(added, removed))
                .child(div().flex_1())
                .child(open_diff),
        )
        .child(column().py(px(4.)).children(rows))
}

fn tree_row(id: ElementId, depth: usize, cx: &App) -> Button {
    Button::new(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .hover(t::hover())
                .foreground(t::secondary()),
        )
        .compact()
        .w_full()
        .h(px(28.))
        .pl(px(12. + 16. * depth as f32))
        .pr(px(12.))
        .rounded(px(0.))
        .border_0()
        .text_size(px(13.))
}
