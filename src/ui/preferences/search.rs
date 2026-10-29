//! Settings search: the index of every page and setting, the query matcher, and
//! one page's results, which the pages use to dim and highlight their rows.
//!
//! Every page in `navigation::GROUPS` brings its entries. A page declares them
//! next to its rendering and draws each row from the same `Entry` (label and
//! help come from it), so the index cannot drift from what the page shows.
use super::navigation;
use super::*;
use std::{cell::Cell, ops::Range, rc::Rc};

/// Space kept above a match scrolled into view.
const REVEAL_MARGIN: f32 = 16.;

/// One searchable setting: a row of a page, or a block of rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Entry {
    /// Unique on its page; the page marks the rendered row with it.
    pub id: &'static str,
    pub section: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    /// Other words people search for: option names, synonyms.
    pub keywords: &'static str,
}
impl Entry {
    pub const fn new(
        id: &'static str,
        section: &'static str,
        label: &'static str,
        help: &'static str,
        keywords: &'static str,
    ) -> Self {
        Self {
            id,
            section,
            label,
            help,
            keywords,
        }
    }
}

/// Lower-case words of the query. A text matches when it contains every word.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Query(Vec<String>);
impl Query {
    pub fn parse(text: &str) -> Self {
        Self(text.split_whitespace().map(str::to_lowercase).collect())
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// `haystack` is already lower case.
    fn matches(&self, haystack: &str) -> bool {
        self.0.iter().all(|word| haystack.contains(word.as_str()))
    }
    /// Byte ranges of the query words in `text`, sorted and merged.
    pub fn ranges(&self, text: &str) -> Vec<Range<usize>> {
        let lower = text.to_lowercase();
        // Case folding that changes lengths would misplace the ranges.
        if lower.len() != text.len() {
            return vec![];
        }
        let mut ranges: Vec<Range<usize>> = self
            .0
            .iter()
            .flat_map(|word| {
                lower
                    .match_indices(word.as_str())
                    .map(|(start, word)| start..start + word.len())
            })
            .filter(|r| text.is_char_boundary(r.start) && text.is_char_boundary(r.end))
            .collect();
        ranges.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for range in ranges {
            match merged.last_mut() {
                Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
                _ => merged.push(range),
            }
        }
        merged
    }
}

struct IndexedPage {
    name: &'static str,
    unavailable: bool,
    /// Name, group, description and keywords of the page, lower case.
    meta: String,
    /// Each entry with its page name, section, label, help and keywords, lower case.
    entries: Vec<(Entry, String)>,
}

fn build_index() -> Vec<IndexedPage> {
    navigation::GROUPS
        .iter()
        .flat_map(|(group, pages)| {
            pages.iter().map(move |page| IndexedPage {
                name: page.name,
                unavailable: navigation::unavailable(page.name),
                meta: format!(
                    "{} {group} {} {}",
                    page.name, page.description, page.keywords
                )
                .to_lowercase(),
                entries: (page.entries)()
                    .into_iter()
                    .map(|e| {
                        let haystack = format!(
                            "{} {} {} {} {}",
                            page.name, e.section, e.label, e.help, e.keywords
                        );
                        (e, haystack.to_lowercase())
                    })
                    .collect(),
            })
        })
        .collect()
}

/// How one page answers the query.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PageResult {
    /// The page's own name, group, description or keywords match.
    pub meta: bool,
    /// Matching entries, in page order.
    pub hits: Vec<&'static str>,
}
impl PageResult {
    pub fn matched(&self) -> bool {
        self.meta || !self.hits.is_empty()
    }
}

fn results(index: &[IndexedPage], query: &Query) -> Vec<PageResult> {
    index
        .iter()
        .map(|page| {
            if query.is_empty() {
                return PageResult::default();
            }
            PageResult {
                meta: query.matches(&page.meta),
                hits: page
                    .entries
                    .iter()
                    .filter(|(_, haystack)| query.matches(haystack))
                    .map(|(entry, _)| entry.id)
                    .collect(),
            }
        })
        .collect()
}

/// The page to show for a query: available pages first, then the most matching
/// settings (a matching page name or description counts as three), then
/// navigation order.
fn best(index: &[IndexedPage], results: &[PageResult]) -> Option<usize> {
    let score = |i: usize| results[i].hits.len() + if results[i].meta { 3 } else { 0 };
    (0..index.len())
        .filter(|&i| results[i].matched())
        .max_by_key(|&i| (!index[i].unavailable, score(i), std::cmp::Reverse(i)))
}

/// The next (or previous) matching page after `current`, wrapping around.
fn step(results: &[PageResult], current: Option<usize>, forward: bool) -> Option<usize> {
    let n = results.len();
    let start = current.unwrap_or(if forward { n.saturating_sub(1) } else { 0 });
    (1..=n)
        .map(|k| {
            if forward {
                (start + k) % n
            } else {
                (start + n - k) % n
            }
        })
        .find(|&i| results[i].matched())
}

/// The query of a Settings window and its results. The page it moves to is the
/// window's `page`.
pub(super) struct SettingsSearch {
    index: Vec<IndexedPage>,
    query: Query,
    text: SharedString,
    results: Vec<PageResult>,
    page: Entity<String>,
    /// Set when search moves to a page with matches; the page's first match
    /// takes it when it is laid out and scrolls itself into view.
    reveal: Rc<Cell<bool>>,
}
impl SettingsSearch {
    pub fn new(page: Entity<String>) -> Self {
        let index = build_index();
        Self {
            results: vec![PageResult::default(); index.len()],
            index,
            query: Query::default(),
            text: SharedString::default(),
            page,
            reveal: Rc::default(),
        }
    }

    /// A new query. When the page shown has no match, moves to the best one.
    pub fn set_query(&mut self, text: &str, cx: &mut Context<Self>) {
        self.text = text.trim().to_owned().into();
        let query = Query::parse(text);
        if query != self.query {
            self.results = results(&self.index, &query);
            self.query = query;
            self.reveal.set(false);
            let current = self.position(cx);
            if self.is_active()
                && !current.is_some_and(|i| self.results[i].matched())
                && let Some(best) = best(&self.index, &self.results)
            {
                self.show(best, cx);
            }
        }
        cx.notify();
    }

    /// Moves to the next (or previous) matching page; false without a query or match.
    pub fn step(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        if !self.is_active() {
            return false;
        }
        let Some(next) = step(&self.results, self.position(cx), forward) else {
            return false;
        };
        self.show(next, cx);
        cx.notify();
        true
    }

    fn position(&self, cx: &App) -> Option<usize> {
        let page = self.page.read(cx);
        self.index.iter().position(|p| p.name == page.as_str())
    }

    fn show(&mut self, index: usize, cx: &mut Context<Self>) {
        let name = self.index[index].name;
        self.reveal.set(!self.results[index].hits.is_empty());
        self.page.update(cx, |page, cx| {
            if page != name {
                *page = name.to_owned();
                cx.notify();
            }
        });
    }

    pub fn is_active(&self) -> bool {
        !self.query.is_empty()
    }
    /// The query as typed, trimmed.
    pub fn text(&self) -> &SharedString {
        &self.text
    }
    /// A query that no page matches.
    pub fn nothing_found(&self) -> bool {
        self.is_active() && !self.results.iter().any(PageResult::matched)
    }
    /// The page's result while a query is active.
    pub fn result(&self, page: &str) -> Option<&PageResult> {
        if !self.is_active() {
            return None;
        }
        let index = self.index.iter().position(|p| p.name == page)?;
        Some(&self.results[index])
    }
    /// How a sidebar entry shows the query: faded without a match (the page
    /// shown keeps full strength), and its number of matching settings.
    pub fn navigation(&self, page: &str, shown: bool) -> (bool, usize) {
        match self.result(page) {
            Some(result) => (!result.matched() && !shown, result.hits.len()),
            None => (false, 0),
        }
    }
    pub fn page(&self, name: &str) -> PageSearch {
        PageSearch {
            query: self.query.clone(),
            hits: self
                .result(name)
                .map(|result| result.hits.clone())
                .unwrap_or_default(),
            reveal: self.reveal.clone(),
            scroll: None,
        }
    }
    /// Results of the page shown now.
    pub fn current(&self, cx: &App) -> PageSearch {
        self.page(self.page.read(cx))
    }
    /// A search for a page tested on its own.
    #[cfg(test)]
    pub fn detached(page: &str, cx: &mut App) -> Entity<Self> {
        let page = cx.new(|_| page.to_owned());
        cx.new(|_| Self::new(page))
    }
}

/// One page's view of the query. While some of its settings match, the others
/// are dimmed and the matched words highlighted; otherwise rows render as usual.
#[derive(Clone, Default)]
pub(super) struct PageSearch {
    query: Query,
    hits: Vec<&'static str>,
    reveal: Rc<Cell<bool>>,
    scroll: Option<ScrollHandle>,
}
impl PageSearch {
    /// The page's scroll area, where its first match is scrolled into view.
    pub fn scroll(mut self, handle: &ScrollHandle) -> Self {
        self.scroll = Some(handle.clone());
        self
    }
    pub fn hit(&self, id: &str) -> bool {
        self.hits.contains(&id)
    }
    fn filtering(&self) -> bool {
        !self.hits.is_empty()
    }
    /// Dims `element` when none of `ids` matches while others on the page do.
    /// The page's first match also scrolls into view after search moved here.
    pub fn mark<E: Styled + ParentElement>(&self, ids: &[&str], element: E) -> E {
        if !self.filtering() {
            return element;
        }
        if !ids.iter().any(|id| self.hit(id)) {
            return element.opacity(t::SEARCH_DIMMED);
        }
        match &self.scroll {
            Some(scroll) if ids.contains(&self.hits[0]) => element
                .relative()
                .child(reveal_anchor(self.reveal.clone(), scroll.clone())),
            _ => element,
        }
    }
    /// `text` of entry `id`, with the query words highlighted when it matches.
    pub fn text(&self, id: &str, text: impl Into<SharedString>) -> StyledText {
        let text = text.into();
        let ranges = if self.hit(id) {
            self.query.ranges(&text)
        } else {
            vec![]
        };
        let style = HighlightStyle {
            background_color: Some(t::search_match()),
            ..Default::default()
        };
        StyledText::new(text).with_highlights(ranges.into_iter().map(|range| (range, style)))
    }
    /// `components::setting` drawn from `entry`.
    pub fn setting(&self, entry: &Entry, first: bool, control: impl IntoElement) -> Div {
        self.setting_with(entry, entry.help, first, control)
    }
    /// A setting whose help depends on the current state.
    pub fn setting_with(
        &self,
        entry: &Entry,
        help: impl Into<SharedString>,
        first: bool,
        control: impl IntoElement,
    ) -> Div {
        self.mark(
            &[entry.id],
            setting(
                self.text(entry.id, entry.label),
                self.text(entry.id, help),
                first,
                control,
            ),
        )
    }
    /// `components::stacked_setting` drawn from `entry`.
    pub fn stacked_setting(
        &self,
        entry: &Entry,
        help: impl Into<SharedString>,
        first: bool,
        control: impl IntoElement,
    ) -> Div {
        self.mark(
            &[entry.id],
            stacked_setting(
                self.text(entry.id, entry.label),
                self.text(entry.id, help),
                first,
                control,
            ),
        )
    }
    /// `components::form_field` drawn from `entry`.
    pub fn field(&self, entry: &Entry, control: impl IntoElement) -> Div {
        self.mark(
            &[entry.id],
            form_field(self.text(entry.id, entry.label), entry.help, control),
        )
    }
}

/// An empty child of the page's first match: once search has moved to the page,
/// scrolls the match into view (unless it is already fully visible).
fn reveal_anchor(pending: Rc<Cell<bool>>, scroll: ScrollHandle) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            if !pending.replace(false) {
                return;
            }
            let viewport = scroll.bounds();
            if bounds.top() >= viewport.top() && bounds.bottom() <= viewport.bottom() {
                return;
            }
            let offset = scroll.offset();
            let y = offset.y - (bounds.top() - viewport.top()) + px(REVEAL_MARGIN);
            scroll.set_offset(point(offset.x, y.min(px(0.))));
            window.on_next_frame(|window, _| window.refresh());
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn index() -> Vec<IndexedPage> {
        build_index()
    }
    fn position(index: &[IndexedPage], name: &str) -> usize {
        index.iter().position(|p| p.name == name).unwrap()
    }

    #[test]
    fn every_word_must_match_in_any_case_across_label_help_keywords_and_page() {
        let query = Query::parse("  Reopen   WORKSPACE ");
        assert!(query.matches("general startup reopen last workspace on startup"));
        assert!(!query.matches("general startup reopen tabs"));
        assert!(Query::parse("   ").is_empty());

        let index = index();
        let found = results(&index, &Query::parse("general startup restore"));
        let general = &found[position(&index, "General")];
        assert_eq!(general.hits, ["reopen"], "page, section and help together");
        // Keywords count: the shell entry mentions zsh only in its keywords.
        let found = results(&index, &Query::parse("zsh"));
        assert!(found[position(&index, "General")].hits.contains(&"shell"));
    }

    #[test]
    fn highlights_merge_overlapping_words_and_skip_unsafe_text() {
        let query = Query::parse("font fon size");
        assert_eq!(query.ranges("Font size of Fonts"), [0..4, 5..9, 13..17]);
        assert!(Query::parse("i").ranges("İstanbul is").is_empty());
    }

    #[test]
    fn every_page_is_indexed_with_unique_entries() {
        let index = index();
        let pages: Vec<&str> = navigation::GROUPS
            .iter()
            .flat_map(|(_, pages)| pages.iter().map(|p| p.name))
            .collect();
        assert_eq!(index.len(), pages.len());
        for page in &index {
            assert!(!page.entries.is_empty(), "{} has no entries", page.name);
            let mut ids: Vec<_> = page.entries.iter().map(|(e, _)| e.id).collect();
            ids.sort_unstable();
            let count = ids.len();
            ids.dedup();
            assert_eq!(ids.len(), count, "{} repeats an entry id", page.name);
            for (entry, _) in &page.entries {
                assert!(
                    !entry.label.trim().is_empty(),
                    "{}: {}",
                    page.name,
                    entry.id
                );
            }
        }
        // Commands come from the keymap: every one is searchable on Shortcuts.
        let shortcuts = &index[position(&index, "Shortcuts")];
        for command in crate::ui::keymap::COMMANDS {
            assert!(
                shortcuts
                    .entries
                    .iter()
                    .any(|(e, _)| e.label == command.name),
                "{} is missing",
                command.name
            );
        }
        let found = results(&index, &Query::parse("split pane"));
        assert_eq!(found[position(&index, "Shortcuts")].hits.len(), 2);
    }

    #[test]
    fn best_page_prefers_available_pages_with_most_matches() {
        let index = index();
        let best_for =
            |text: &str| best(&index, &results(&index, &Query::parse(text))).map(|i| index[i].name);
        assert_eq!(best_for("font size"), Some("Appearance"));
        assert_eq!(best_for("notch"), Some("Notch"));
        assert_eq!(best_for("listen address"), Some("Connections"));
        // Greyed pages stay searchable, but an available match is shown first.
        assert_eq!(best_for("telemetry"), Some("Privacy"));
        assert_eq!(best_for("worktree"), Some("General"));
        assert_eq!(best_for("qwertyuiop"), None);
    }

    #[test]
    fn stepping_cycles_through_matching_pages_in_navigation_order() {
        let index = index();
        let found = results(&index, &Query::parse("api key"));
        let names = |i: Option<usize>| i.map(|i| index[i].name);
        let claude = position(&index, "Claude");
        let next = step(&found, Some(claude), true);
        assert_eq!(names(next), Some("Gemini"));
        assert_eq!(names(step(&found, next, false)), Some("Claude"));
        // From a page without a match: the next match after it.
        assert_eq!(
            names(step(&found, Some(position(&index, "General")), true)),
            Some("Claude")
        );
        let last = (0..index.len()).rev().find(|&i| found[i].matched());
        assert_eq!(
            step(&found, last, true),
            (0..index.len()).find(|&i| found[i].matched())
        );
        assert_eq!(
            step(&results(&index, &Query::parse("qwertyuiop")), Some(0), true),
            None
        );
    }
}
