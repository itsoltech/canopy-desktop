//! Owned, selectable Markdown with background preparation and Canopy styles.
use super::{components::*, theme as t};
use canopy_desktop::markdown::Segment;
use gpui_kit::{
    base::{TextView, TextViewState, TextViewStyle},
    *,
};
use std::{path::PathBuf, sync::Arc};

pub(crate) mod attachment_badge;
pub(crate) mod file_pill;
mod highlight;

/// Draws attachment badge `index` at a line's height (a sent message's attachments).
/// Plugins prepare text off the UI thread, so the renderer is shareable.
pub(crate) type BadgeRenderer =
    Arc<dyn Fn(usize, Pixels, &mut Window, &mut App) -> AnyElement + Send + Sync>;

/// Syntax highlighting of a code block outside Markdown (tool previews).
pub(crate) fn highlight_code(
    block: &gpui_kit::base::text::CodeBlock,
    cx: &App,
) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    highlight::shared(cx)(block)
}

/// Chat replies are shown as prose and code parts; each has its own text view, so
/// selection still runs across them while code gets a header and line numbers.
enum Part {
    Prose(Entity<TextViewState>),
    Code {
        lang: Option<String>,
        code: SharedString,
        text: Entity<TextViewState>,
        lines: usize,
        /// Longest line in characters, for the horizontal scroll width.
        longest: usize,
    },
}

pub struct MarkdownView {
    text: Entity<TextViewState>,
    reading: bool,
    keep_previous: bool,
    /// Agent replies: streamed text fades in, code blocks get a copy action and an
    /// empty or preparing block renders nothing instead of a placeholder.
    chat: bool,
    /// A document to read (an agent's plan): sections and code stand out in color.
    document: bool,
    /// A thinking summary: smaller and muted, under its header.
    quiet: bool,
    stream_fade: bool,
    /// The text is final: file mentions inside `files` (the chat's folder) become pills.
    done: bool,
    files: Option<PathBuf>,
    source: Arc<str>,
    base_url: String,
    interactive: bool,
    loading: bool,
    revision: u64,
    prepare: Option<Task<()>>,
    parts: Vec<Part>,
    /// The text each part was last given, so unchanged parts are not re-parsed.
    part_texts: Vec<String>,
    /// Attachment links become badges drawn by this.
    badges: Option<BadgeRenderer>,
    _observer: Subscription,
}
impl MarkdownView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let text = cx.new(|cx| TextViewState::markdown("", cx));
        let observer = cx.observe(&text, |_, _, cx| cx.notify());
        Self {
            text,
            reading: false,
            keep_previous: false,
            chat: false,
            document: false,
            quiet: false,
            stream_fade: false,
            done: false,
            files: None,
            source: Arc::from(""),
            base_url: String::new(),
            interactive: false,
            loading: false,
            revision: 0,
            prepare: None,
            parts: vec![],
            part_texts: vec![],
            badges: None,
            _observer: observer,
        }
    }
    pub fn live(cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(cx);
        view.keep_previous = true;
        view
    }
    pub fn reading(cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(cx);
        view.reading = true;
        view
    }
    pub fn chat(cx: &mut Context<Self>) -> Self {
        let mut view = Self::live(cx);
        view.reading = true;
        view.chat = true;
        view.interactive = true;
        view
    }
    /// An agent's plan: chat rendering with colored headings and code spans, so its
    /// structure stands apart from the interface around it.
    pub fn document(cx: &mut Context<Self>) -> Self {
        let mut view = Self::chat(cx);
        view.document = true;
        view
    }
    /// An agent's thinking: chat rendering, smaller and muted.
    pub fn thinking(cx: &mut Context<Self>) -> Self {
        let mut view = Self::chat(cx);
        view.quiet = true;
        view
    }
    /// The text views it renders (prose and code, in reading order): what search and
    /// highlights work on.
    pub fn text_views(&self) -> Vec<Entity<TextViewState>> {
        if !self.chat {
            return vec![self.text.clone()];
        }
        self.parts
            .iter()
            .map(|part| match part {
                Part::Prose(state) | Part::Code { text: state, .. } => state.clone(),
            })
            .collect()
    }
    pub(crate) fn set_badges(&mut self, renderer: BadgeRenderer) {
        self.badges = Some(renderer);
    }

    /// Streams agent text; the policy is resolved here, outside render.
    pub fn stream(&mut self, source: Arc<str>, cx: &mut Context<Self>) {
        self.stream_fade =
            canopy_desktop::motion::policy(cx) == canopy_desktop::motion::MotionPolicy::Full;
        self.set_content(source, String::new(), true, cx);
    }
    /// The reply is complete: link mentions of existing files under `files`, once.
    pub fn finish(&mut self, files: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.done {
            return;
        }
        self.done = true;
        self.files = files;
        if self.files.is_some() && !self.source.trim().is_empty() {
            self.revision = self.revision.wrapping_add(1);
            self.prepare(cx);
        }
    }
    pub fn set_content(
        &mut self,
        source: Arc<str>,
        base_url: String,
        interactive: bool,
        cx: &mut Context<Self>,
    ) {
        if self.interactive != interactive {
            self.interactive = interactive;
            cx.notify();
        }
        if self.source == source && self.base_url == base_url {
            return;
        }
        let has_previous = !self.source.trim().is_empty() && !self.loading;
        self.source = source.clone();
        self.base_url = base_url.clone();
        self.revision = self.revision.wrapping_add(1);
        self.prepare = None;
        if !self.keep_previous || source.trim().is_empty() {
            self.text.update(cx, |state, cx| state.set_text("", cx));
        }
        self.loading = !(source.trim().is_empty() || self.keep_previous && has_previous);
        if !source.trim().is_empty() {
            self.prepare(cx);
        }
        cx.notify();
    }

    /// Prepares the current source off the UI thread; later revisions win.
    fn prepare(&mut self, cx: &mut Context<Self>) {
        let source = self.source.clone();
        let base_url = self.base_url.clone();
        let files = self.files.clone().filter(|_| self.done);
        let revision = self.revision;
        let chat = self.chat;
        self.prepare = Some(cx.spawn(async move |this, cx| {
            let (prepared, segments) = cx
                .background_executor()
                .spawn(async move {
                    let prepared = canopy_desktop::markdown::prepare(&source, &base_url);
                    let prepared = match files {
                        Some(root) => canopy_desktop::chat::mentions::link_files(&prepared, &root),
                        None => prepared,
                    };
                    let segments = chat.then(|| canopy_desktop::markdown::split_code(&prepared));
                    (prepared, segments)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.revision != revision {
                    return;
                }
                match segments {
                    Some(segments) => this.apply_segments(segments, cx),
                    None => this
                        .text
                        .update(cx, |state, cx| state.set_text(&prepared, cx)),
                }
                this.loading = false;
                cx.notify();
            });
        }));
    }

    /// Updates parts in place, so a streaming reply grows instead of being rebuilt.
    fn apply_segments(&mut self, segments: Vec<Segment>, cx: &mut Context<Self>) {
        let len = segments.len();
        for (index, segment) in segments.into_iter().enumerate() {
            let reuse = match (self.parts.get(index), &segment) {
                (Some(Part::Prose(_)), Segment::Prose(_)) => true,
                (Some(Part::Code { lang, .. }), Segment::Code { lang: new, .. }) => lang == new,
                _ => false,
            };
            let text = match &segment {
                Segment::Prose(prose) => prose.clone(),
                Segment::Code { lang, code } => fenced(lang.as_deref(), code),
            };
            if reuse {
                if self.part_texts.get(index) == Some(&text) {
                    continue;
                }
                let state = match &self.parts[index] {
                    Part::Prose(state) | Part::Code { text: state, .. } => state.clone(),
                };
                state.update(cx, |state, cx| state.set_text(&text, cx));
            } else {
                let state = cx.new(|cx| TextViewState::markdown(&text, cx));
                let observer = cx.observe(&state, |_, _, cx| cx.notify());
                // Parts live as long as the view; the observer goes with the entity.
                observer.detach();
                let part = match &segment {
                    Segment::Prose(_) => Part::Prose(state),
                    Segment::Code { lang, .. } => Part::Code {
                        lang: lang.clone(),
                        code: SharedString::default(),
                        text: state,
                        lines: 0,
                        longest: 0,
                    },
                };
                if index < self.parts.len() {
                    self.parts[index] = part;
                } else {
                    self.parts.push(part);
                }
            }
            if let (
                Part::Code {
                    code,
                    lines,
                    longest,
                    ..
                },
                Segment::Code { code: new, .. },
            ) = (&mut self.parts[index], &segment)
            {
                *lines = new.lines().count().max(1);
                *longest = new
                    .lines()
                    .map(canopy_desktop::markdown::columns)
                    .max()
                    .unwrap_or(0);
                *code = new.clone().into();
            }
            if index < self.part_texts.len() {
                self.part_texts[index] = text;
            } else {
                self.part_texts.push(text);
            }
        }
        self.parts.truncate(len);
        self.part_texts.truncate(len);
    }
}
fn markdown_style(reading: bool, chat: bool) -> TextViewStyle {
    let code = if chat {
        // A card whose top edge holds the language and Copy (see `code_header`).
        div()
            .id("markdown-code-style")
            .overflow_x_scroll()
            .pt(px(CODE_HEADER))
            .px(px(14.))
            .pb(px(12.))
            .rounded(px(8.))
            .border_1()
            .border_color(t::border())
            .font_family(t::MONO)
            .text_size(px(12.5))
            .line_height(px(19.))
    } else {
        div()
            .id("markdown-code-style")
            .overflow_x_scroll()
            .p(px(8.))
            .rounded(px(4.))
            .font_family(t::MONO)
            .text_size(px(if reading { 13. } else { 12. }))
            .line_height(px(if reading { 20. } else { 18. }))
    }
    .style()
    .clone();
    let table = div()
        .id("markdown-table-style")
        .overflow_x_scroll()
        .rounded(px(4.))
        .style()
        .clone();
    TextViewStyle::default()
        .with_dark(!t::is_light())
        .with_foreground(t::text())
        .with_muted_foreground(t::secondary())
        .with_link(t::accent())
        .with_selection(t::accent().opacity(0.22))
        .with_border(t::control_border())
        .with_code_background(if chat { t::elevated() } else { t::hover() })
        .with_paragraph_gap(rems(0.65))
        .with_heading(move |level| {
            StyleRefinement::default().text_size(px(if reading {
                match level {
                    1 => 20.,
                    2 => 18.,
                    3 => 16.,
                    _ => 14.,
                }
            } else {
                match level {
                    1 => 16.,
                    2 => 15.,
                    3 => 14.,
                    _ => 13.,
                }
            }))
        })
        .with_inline_code(HighlightStyle {
            color: Some(t::text()),
            background_color: Some(t::hover()),
            ..Default::default()
        })
        .with_code_block(code)
        .with_table(table)
        .with_table_cell(div().px(px(8.)).py(px(6.)).style().clone())
}
/// Chat style with a reading hierarchy: an underlined title, accent section headings,
/// tinted code spans and quotes on an accent rule.
fn document_style() -> TextViewStyle {
    markdown_style(true, true)
        .with_muted_foreground(t::secondary())
        .with_border(t::accent().opacity(0.45))
        .with_paragraph_gap(rems(0.8))
        .with_heading(|level| {
            let heading = StyleRefinement::default();
            match level {
                1 => heading
                    .text_size(px(19.))
                    .text_color(t::text())
                    .mt(px(2.))
                    .pb(px(8.))
                    .border_b_1()
                    .border_color(t::border()),
                2 => heading
                    .text_size(px(15.))
                    .text_color(t::accent())
                    .mt(px(10.)),
                3 => heading
                    .text_size(px(14.))
                    .text_color(t::yellow())
                    .mt(px(6.)),
                _ => heading.text_size(px(13.5)).text_color(t::secondary()),
            }
        })
        .with_inline_code(HighlightStyle {
            color: Some(t::accent()),
            background_color: Some(t::accent().opacity(0.12)),
            ..Default::default()
        })
}

/// Room above the code for the language label and Copy.
const CODE_HEADER: f32 = 36.;

/// Language and Copy, placed by the text view in the code block's top-right corner.
fn code_header(block: &gpui_kit::base::text::CodeBlock) -> Div {
    let language = block
        .lang()
        .and_then(|tag| {
            tag.split([',', ' ', '{', '}'])
                .map(|word| word.trim_start_matches('.'))
                .find(|word| !word.is_empty())
                .map(str::to_ascii_lowercase)
        })
        .unwrap_or_else(|| "Code".into());
    let code = block.code();
    let id = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        code.hash(&mut hasher);
        hasher.finish()
    };
    row()
        .h(px(22.))
        .gap(px(6.))
        .pl(px(8.))
        .child(
            div()
                .text_size(px(11.))
                .font_family(super::platform_ui::interface_font())
                .text_color(t::muted())
                .child(language),
        )
        .child(
            gpui_kit::component::clipboard::Clipboard::new(("copy-code", id))
                .value(code)
                .tooltip("Copy"),
        )
}

impl MarkdownView {
    fn motion(&self) -> gpui_kit::base::TextViewMotion {
        if self.stream_fade {
            gpui_kit::base::TextViewMotion::default()
                .with_stream_fade(canopy_desktop::motion::duration::MEDIUM)
                .with_stream_fade_stagger(canopy_desktop::motion::duration::STAGGER)
        } else {
            gpui_kit::base::TextViewMotion::default()
        }
    }

    /// Prose with Canopy's Markdown style, links and (in chat) file pills.
    /// `part` tells apart the prose parts of one reply (pill ids).
    fn text_view(
        &self,
        state: &Entity<TextViewState>,
        part: usize,
        cx: &mut Context<Self>,
    ) -> TextView {
        let base = self.base_url.clone();
        let interactive = self.interactive;
        let mut view = TextView::new(state).motion(self.motion());
        if self.chat {
            // Code nested in lists or quotes keeps the built-in block.
            view = view
                .code_block_actions(|block, _, _| code_header(block))
                .shared_code_block_highlighter(highlight::shared(cx));
            if let Some(renderer) = &self.badges {
                view = view.plugin(attachment_badge::AttachmentBadges {
                    renderer: renderer.clone(),
                });
            }
            if let Some(root) = &self.files {
                view = view.plugin(file_pill::FilePills {
                    root: root.clone(),
                    owner: cx.entity_id().as_u64(),
                    part,
                });
            }
        }
        let style = if self.document {
            document_style()
        } else if self.quiet {
            markdown_style(self.reading, self.chat)
                .with_foreground(t::muted())
                .with_paragraph_gap(rems(0.5))
        } else {
            markdown_style(self.reading, self.chat)
        };
        let (size, line) = match (self.quiet, self.reading) {
            (true, _) => (13., 20.),
            (false, true) => (14., 22.),
            (false, false) => (12., 19.),
        };
        view.style(style)
            .w_full()
            .min_w_0()
            .text_size(px(size))
            .line_height(px(line))
            .scrollable(false)
            .selectable(interactive)
            .on_link_click(move |target, event, _, cx| {
                let activate = match event {
                    ClickEvent::Mouse(click) => {
                        matches!(click.up.button, MouseButton::Left | MouseButton::Middle)
                    }
                    ClickEvent::Keyboard(_) => true,
                    ClickEvent::Touch(click) => !click.long_press,
                };
                if interactive
                    && activate
                    && let Some(url) = canopy_desktop::markdown::link_target(target, &base)
                {
                    cx.open_url(&url);
                }
            })
    }

    /// A top-level code block: header with language and Copy, line numbers, and the
    /// code in its own text view (selectable with the prose) that scrolls sideways.
    fn code_card(&self, index: usize, part: &Part, advance: Pixels, cx: &mut Context<Self>) -> Div {
        let Part::Code {
            lang,
            code,
            text,
            lines,
            longest,
        } = part
        else {
            return div();
        };
        let label = lang
            .as_deref()
            .and_then(|tag| {
                tag.split([',', ' ', '{', '}'])
                    .map(|word| word.trim_start_matches('.'))
                    .filter(|word| !word.is_empty())
                    .map(str::to_ascii_lowercase)
                    .next()
            })
            .unwrap_or_else(|| "code".into());
        let owner = cx.entity_id().as_u64();
        // Two spare columns: glyph fallbacks may be a little wider than the estimate.
        let width = advance * (*longest as f32 + 2.) + px(8.);
        column()
            .w_full()
            .min_w_0()
            .rounded(px(8.))
            .border_1()
            .border_color(t::border())
            .bg(t::elevated())
            .overflow_hidden()
            .child(
                row()
                    .h(px(32.))
                    .px(px(12.))
                    .gap(px(8.))
                    .border_b_1()
                    .border_color(t::border())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(t::muted())
                            .child(label),
                    )
                    .child(
                        gpui_kit::component::clipboard::Clipboard::new((
                            "copy-code",
                            element_key(owner, index, 0),
                        ))
                        .value(code.clone())
                        .tooltip("Copy"),
                    ),
            )
            .child(
                row()
                    .w_full()
                    .items_start()
                    .child(
                        column()
                            .flex_shrink_0()
                            .py(px(10.))
                            .pl(px(12.))
                            .pr(px(10.))
                            .items_end()
                            .font_family(t::MONO)
                            .text_size(px(CODE_SIZE))
                            .line_height(px(CODE_LINE))
                            .text_color(t::faint())
                            .children(
                                (1..=*lines)
                                    .map(|line| div().h(px(CODE_LINE)).child(line.to_string())),
                            ),
                    )
                    .child(
                        div()
                            .id(("code-scroll", element_key(owner, index, 0)))
                            .flex_1()
                            .min_w_0()
                            .overflow_x_scroll()
                            .py(px(10.))
                            .pr(px(14.))
                            .child(
                                TextView::new(text)
                                    .motion(self.motion())
                                    .shared_code_block_highlighter(highlight::shared(cx))
                                    .style(code_style())
                                    .w(width)
                                    .min_w_full()
                                    .text_size(px(CODE_SIZE))
                                    .line_height(px(CODE_LINE))
                                    .scrollable(false)
                                    .selectable(self.interactive),
                            ),
                    ),
            )
    }
}

impl Render for MarkdownView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.chat && (self.loading || self.source.trim().is_empty()) {
            return div().into_any_element();
        }
        if self.loading {
            return column()
                .gap(px(8.))
                .child(skeleton_bar().w_full().h(px(10.)))
                .child(skeleton_bar().w(px(112.)).h(px(10.)))
                .into_any_element();
        }
        if self.source.trim().is_empty() {
            return div()
                .text_size(px(12.))
                .text_color(t::muted())
                .child("No description provided.")
                .into_any_element();
        }
        if self.chat {
            let advance = {
                let text = window.text_system();
                let font = text.resolve_font(&font(t::MONO));
                text.em_advance(font, px(CODE_SIZE))
                    .unwrap_or(px(CODE_SIZE * 0.6))
            };
            let parts: Vec<AnyElement> = (0..self.parts.len())
                .map(|index| match &self.parts[index] {
                    Part::Prose(state) => {
                        let state = state.clone();
                        self.text_view(&state, index, cx).into_any_element()
                    }
                    part @ Part::Code { .. } => {
                        self.code_card(index, part, advance, cx).into_any_element()
                    }
                })
                .collect();
            return column()
                .w_full()
                .min_w_0()
                .gap(px(12.))
                .children(parts)
                .into_any_element();
        }
        let text = self.text.clone();
        self.text_view(&text, 0, cx).into_any_element()
    }
}

/// Element ids for parts of one reply: the view, the part and an offset in it.
pub(crate) fn element_key(owner: u64, part: usize, offset: usize) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (owner, part, offset).hash(&mut hasher);
    hasher.finish()
}

const CODE_SIZE: f32 = 12.5;
const CODE_LINE: f32 = 19.;

/// The code inside a code card: no own background, padding or wrapping.
fn code_style() -> TextViewStyle {
    TextViewStyle::default()
        .with_dark(!t::is_light())
        .with_foreground(t::text())
        .with_muted_foreground(t::secondary())
        .with_selection(t::accent().opacity(0.22))
        .with_code_background(hsla(0., 0., 0., 0.))
        .with_paragraph_gap(rems(0.))
        .with_code_block(
            div()
                .p_0()
                .rounded(px(0.))
                .font_family(t::MONO)
                .text_size(px(CODE_SIZE))
                .line_height(px(CODE_LINE))
                .whitespace_nowrap()
                .style()
                .clone(),
        )
}

/// A fenced block that keeps `code` intact, whatever backtick runs it contains.
fn fenced(lang: Option<&str>, code: &str) -> String {
    let mut fence = "```".to_owned();
    while code.contains(fence.as_str()) {
        fence.push('`');
    }
    let lang: String = lang
        .unwrap_or_default()
        .chars()
        .filter(|c| !matches!(c, '`' | '\n' | '\r'))
        .collect();
    format!("{fence}{lang}\n{code}\n{fence}")
}
