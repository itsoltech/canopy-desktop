//! Find in the conversation (DEC-069): ⌘F opens a bar over the shown rows; matches are
//! range highlights on the rows' text views and the current one is scrolled to,
//! opening the fold, group or thinking that hides it.
use super::{rows::Rows, view::ChatView};
use crate::ui::{components::*, theme as t};
use canopy_desktop::motion::{self, Presence, presets};
use gpui_kit::base::{Disableable, RangeHighlight, TextViewState};
use gpui_kit::component::{
    IconName,
    input::{InputEvent, InputState},
};
use gpui_kit::*;
use std::{
    collections::HashMap,
    ops::Range,
    time::{Duration, Instant},
};

/// Content changes (streaming) are searched again after this pause.
const SETTLE: Duration = Duration::from_millis(150);
/// How long the source of a clicked quote stays lit.
const FLASH: Duration = Duration::from_millis(1500);

/// Non-overlapping, case-insensitive occurrences of `needle` in `haystack`, as byte
/// ranges of `haystack` on whole characters.
pub(super) fn matches(haystack: &str, needle: &str) -> Vec<Range<usize>> {
    let needle: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return vec![];
    }
    // Lowercased characters with the source character each comes from.
    let lowered: Vec<(char, usize, usize)> = haystack
        .char_indices()
        .flat_map(|(start, c)| {
            let end = start + c.len_utf8();
            c.to_lowercase().map(move |lower| (lower, start, end))
        })
        .collect();
    let mut found = vec![];
    let mut at = 0;
    while at + needle.len() <= lowered.len() {
        let end = at + needle.len();
        let equal = lowered[at..end]
            .iter()
            .map(|(c, ..)| *c)
            .eq(needle.iter().copied());
        // A match covers whole source characters, not part of one's lowercase form.
        let whole = (at == 0 || lowered[at - 1].1 != lowered[at].1)
            && (end == lowered.len() || lowered[end].1 != lowered[end - 1].1);
        if equal && whole {
            found.push(lowered[at].1..lowered[end - 1].2);
            at = end;
        } else {
            at += 1;
        }
    }
    found
}

/// One occurrence: the row, which of its text views, and where in its rendered text.
#[derive(Clone)]
pub(super) struct Hit {
    pub key: u64,
    pub part: usize,
    pub view: Entity<TextViewState>,
    pub range: Range<usize>,
}

/// Every occurrence of `query` in the rows' text, in reading order.
fn search(rows: &Rows, query: &str, cx: &App) -> Vec<Hit> {
    let mut hits = vec![];
    for row in &rows.rows {
        let Some(markdown) = &row.markdown else {
            continue;
        };
        for (part, view) in markdown.read(cx).text_views().into_iter().enumerate() {
            let text = view.read(cx).rendered_text();
            for range in matches(text.as_str(), query) {
                hits.push(Hit {
                    key: row.key,
                    part,
                    view: view.clone(),
                    range,
                });
            }
        }
    }
    hits
}

/// The ⌘F bar and its results.
pub(super) struct Find {
    pub input: Entity<InputState>,
    pub hits: Vec<Hit>,
    pub current: Option<usize>,
    /// Shown (false while it fades out after closing).
    pub open: bool,
    pub presence: Presence,
    refresh: Option<Task<()>>,
    _events: Subscription,
    remote: Vec<canopy_desktop::remote::proto::HistoryMatch>,
    remote_current: Option<usize>,
    remote_next: Option<String>,
    remote_loading: bool,
    remote_target: Option<canopy_desktop::remote::proto::ItemKey>,
    remote_generation: u64,
}

/// The source of a clicked quote, lit for a moment.
pub(super) struct Flash {
    pub hit: Hit,
    _fade: Task<()>,
}

impl ChatView {
    /// ⌘F: opens the bar, or selects its query when it is already open.
    pub(super) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_quote_offer(cx);
        if let Some(find) = self.find.as_mut().filter(|find| find.open) {
            find.input.update(cx, |input, cx| {
                input.focus(window, cx);
                input.select_all(window, cx);
            });
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Find in chat"));
        let events =
            cx.subscribe_in(
                &input,
                window,
                |this, _, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => this.search_now(true, cx),
                    InputEvent::PressEnter { shift, .. } => {
                        this.find_step(if *shift { -1 } else { 1 }, cx)
                    }
                    _ => {}
                },
            );
        input.update(cx, |input, cx| input.focus(window, cx));
        let now = Instant::now();
        let mut presence = Presence::new(false, presets::POPOVER, now);
        presence.set_open(true, now, motion::policy(cx));
        self.find = Some(Find {
            input,
            hits: vec![],
            current: None,
            open: true,
            presence,
            refresh: None,
            _events: events,
            remote: vec![],
            remote_current: None,
            remote_next: None,
            remote_loading: false,
            remote_target: None,
            remote_generation: 0,
        });
        cx.notify();
    }

    /// Esc or ×: the highlights go and the composer gets the keys back.
    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(find) = self.find.as_mut().filter(|find| find.open) else {
            return false;
        };
        find.open = false;
        find.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        find.hits.clear();
        find.current = None;
        find.refresh = None;
        self.apply_highlights(cx);
        if self.threads.is_empty() {
            let composer = self.composer.clone();
            composer.update(cx, |state, cx| state.focus(window, cx));
        } else {
            self.focus.focus(window, cx);
        }
        cx.notify();
        true
    }

    fn query(&self, cx: &App) -> Option<String> {
        let find = self.find.as_ref().filter(|find| find.open)?;
        Some(find.input.read(cx).value().to_string())
    }

    /// Searches the shown rows again. A new query starts at the first match from the
    /// top of the view; changed content keeps the current match where it is.
    fn search_now(&mut self, new_query: bool, cx: &mut Context<Self>) {
        let Some(query) = self.query(cx) else {
            return;
        };
        if new_query && self.rt(cx).is_remote() {
            self.search_remote(query.clone(), None, cx);
        }
        let rows = self.shown_rows();
        let hits = search(rows, &query, cx);
        let order: HashMap<u64, usize> = rows
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| (row.key, index))
            .collect();
        let top = rows.scroller.read(cx).top_row();
        let find = self.find.as_mut().expect("find is open");
        let previous = find
            .current
            .and_then(|current| find.hits.get(current))
            .cloned();
        let position = |hit: &Hit| {
            (
                order.get(&hit.key).copied().unwrap_or(0),
                hit.part,
                hit.range.start,
            )
        };
        let current = if hits.is_empty() {
            None
        } else if let (false, Some(previous)) = (new_query, previous) {
            let at = position(&previous);
            Some(
                hits.iter()
                    .position(|hit| position(hit) >= at)
                    .unwrap_or(hits.len() - 1),
            )
        } else {
            Some(
                hits.iter()
                    .position(|hit| position(hit).0 >= top)
                    .unwrap_or(0),
            )
        };
        find.hits = hits;
        find.current = current;
        if new_query && let Some(current) = current {
            self.reveal_hit(current, cx);
        }
        self.apply_highlights(cx);
        self.reveal_remote_target(cx);
        cx.notify();
    }

    /// The conversation changed under an open bar: search again once it settles.
    pub(super) fn refresh_find(&mut self, cx: &mut Context<Self>) {
        let Some(find) = self.find.as_mut().filter(|find| find.open) else {
            return;
        };
        find.refresh = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            let _ = this.update(cx, |this, cx| this.search_now(false, cx));
        }));
    }

    /// Enter / ⌘G (`1`), ⇧Enter / ⇧⌘G (`-1`).
    pub(super) fn find_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.rt(cx).is_remote() && self.find.as_ref().is_some_and(|f| !f.remote.is_empty()) {
            let find = self.find.as_mut().unwrap();
            let current = find.remote_current.unwrap_or(0);
            if delta > 0 && current + 1 == find.remote.len() && find.remote_next.is_some() {
                let token = find.remote_next.clone();
                let query = find.input.read(cx).value().to_string();
                self.search_remote(query, token, cx);
                return;
            }
            let next = (current as isize + delta).rem_euclid(find.remote.len() as isize) as usize;
            find.remote_current = Some(next);
            find.remote_target = find.remote[next].key;
            let target = find.remote_target;
            if !self.reveal_remote_target(cx) {
                self.runtime
                    .update(cx, |r, cx| r.client_history(target, cx));
            }
            return;
        }
        let Some(find) = self.find.as_mut().filter(|find| find.open) else {
            return;
        };
        let count = find.hits.len() as isize;
        if count == 0 {
            return;
        }
        let next = match find.current {
            Some(current) => (current as isize + delta).rem_euclid(count),
            None => 0,
        } as usize;
        find.current = Some(next);
        self.reveal_hit(next, cx);
        self.apply_highlights(cx);
        cx.notify();
    }

    fn search_remote(&mut self, query: String, token: Option<String>, cx: &mut Context<Self>) {
        let Some(source) = self.rt(cx).remote_source() else {
            return;
        };
        let Some(host) = source.host.clone() else {
            return;
        };
        let chat = source.chat();
        let Some(find) = self.find.as_mut() else {
            return;
        };
        if token.is_none() {
            find.remote.clear();
            find.remote_current = None;
            find.remote_next = None;
            find.remote_target = None;
        }
        find.remote_loading = true;
        find.remote_generation = find.remote_generation.wrapping_add(1);
        let generation = find.remote_generation;
        if query.trim().is_empty() {
            find.remote_loading = false;
            return;
        }
        let task = crate::ui::remote_client::executor(cx).unwrap().spawn({
            let query = query.clone();
            async move {
                canopy_desktop::remote::proto::chat_service_client::ChatServiceClient::new(
                    host.channel()?,
                )
                .search_history(canopy_desktop::remote::proto::SearchHistoryRequest {
                    chat: Some(chat),
                    text: query,
                    page_token: token,
                })
                .await
                .map(|r| r.into_inner())
                .map_err(canopy_desktop::remote::client::rpc_error)
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await.unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                let Some(find) = this.find.as_mut().filter(|f| {
                    f.open
                        && f.remote_generation == generation
                        && f.input.read(cx).value().as_ref() == query
                }) else {
                    return;
                };
                find.remote_loading = false;
                match result {
                    Ok(page) => {
                        let first = find.remote.len();
                        find.remote.extend(page.matches);
                        find.remote_next = page.next_page_token;
                        find.remote_current = (first < find.remote.len()).then_some(first);
                        find.remote_target = find.remote.get(first).and_then(|m| m.key);
                        let target = find.remote_target;
                        if !this.reveal_remote_target(cx) && target.is_some() {
                            this.runtime
                                .update(cx, |r, cx| r.client_history(target, cx));
                        }
                    }
                    Err(error) => this.shell_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn reveal_remote_target(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(target) = self.find.as_ref().and_then(|f| f.remote_target) else {
            return false;
        };
        let rows = self.shown_rows();
        let transcript = self.transcript(cx);
        let row = rows.rows.iter().find_map(|row| {
            let super::rows::Kind::Item(source) = row.kind else {
                return None;
            };
            let epoch = match source {
                super::rows::Source::Current(_) => self.rt(cx).epoch(),
                super::rows::Source::Archived { epoch, .. } => rows.archived.get(epoch)?.epoch,
            };
            (epoch == target.history_epoch && rows.item(source, transcript)?.id.0 == target.item_id)
                .then_some(row.key)
        });
        let hit = row.and_then(|key| {
            self.find
                .as_ref()?
                .hits
                .iter()
                .find(|hit| hit.key == key)
                .cloned()
        });
        if let Some(hit) = hit {
            self.reveal(&hit, cx);
            self.find.as_mut().unwrap().remote_target = None;
            cx.notify();
            return true;
        }
        false
    }

    fn reveal_hit(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(hit) = self
            .find
            .as_ref()
            .and_then(|find| find.hits.get(index))
            .cloned()
        else {
            return;
        };
        self.reveal(&hit, cx);
    }

    /// Opens what hides the hit's row, brings the row into view and its line with it.
    fn reveal(&mut self, hit: &Hit, cx: &mut Context<Self>) {
        let rows = self.shown_rows_mut();
        let Some(row) = rows.rows.iter().position(|row| row.key == hit.key) else {
            return;
        };
        rows.uncover(row, cx);
        rows.scroller
            .update(cx, |state, cx| state.reveal_row(row, cx));
        let range = hit.range.clone();
        hit.view.update(cx, |view, cx| {
            let _ = view.reveal_range(range, cx);
        });
    }

    /// Puts the find results and a lit quote source on the text views; views lit
    /// before and not now are cleared.
    pub(super) fn apply_highlights(&mut self, cx: &mut Context<Self>) {
        let mut lit: Vec<(Entity<TextViewState>, Vec<RangeHighlight>)> = vec![];
        let mut add = |view: &Entity<TextViewState>, highlight: RangeHighlight| match lit
            .iter_mut()
            .find(|(lit, _)| lit == view)
        {
            Some((_, highlights)) => highlights.push(highlight),
            None => lit.push((view.clone(), vec![highlight])),
        };
        if let Some(find) = self.find.as_ref().filter(|find| find.open) {
            for (index, hit) in find.hits.iter().enumerate() {
                let alpha = if find.current == Some(index) {
                    0.6
                } else {
                    0.25
                };
                add(
                    &hit.view,
                    RangeHighlight::new(hit.range.clone(), t::yellow().opacity(alpha)),
                );
            }
        }
        if let Some(flash) = &self.flash {
            add(
                &flash.hit.view,
                RangeHighlight::new(flash.hit.range.clone(), t::accent().opacity(0.35)),
            );
        }
        for view in std::mem::take(&mut self.lit) {
            if !lit.iter().any(|(lit, _)| *lit == view) {
                view.update(cx, |view, cx| view.clear_range_highlights(cx));
            }
        }
        for (view, highlights) in &lit {
            // A view whose text changed since the search rejects stale ranges; the
            // next search sets them again.
            if view
                .update(cx, |view, cx| {
                    view.set_range_highlights(highlights.clone(), cx)
                })
                .is_err()
            {
                view.update(cx, |view, cx| view.clear_range_highlights(cx));
            }
        }
        self.lit = lit.into_iter().map(|(view, _)| view).collect();
    }

    /// Scrolls to the first line of `quote` in the shown conversation, above the
    /// message that sent it when known (else its latest occurrence), and lights it.
    pub(super) fn show_quote(&mut self, quote: &str, message: Option<u64>, cx: &mut Context<Self>) {
        let Some(line) = quote.lines().map(str::trim).find(|line| !line.is_empty()) else {
            return;
        };
        let line: String = line.chars().take(200).collect();
        let rows = self.shown_rows();
        let quoted_in = |row: &super::rows::Row| {
            let transcript = self.transcript(cx);
            let super::rows::Kind::Item(source) = row.kind else {
                return false;
            };
            rows.item(source, transcript).is_some_and(|item| {
                matches!(&item.body, canopy_desktop::chat::transcript::Body::User(m)
                    if m.attachments.iter().any(|a| matches!(a,
                        canopy_desktop::chat::transcript::Attachment::Quote { text, .. } if text == quote)))
            })
        };
        let before = match message {
            Some(key) => rows.rows.iter().position(|row| row.key == key),
            None => rows.rows.iter().rposition(quoted_in),
        };
        let keys: Vec<u64> = rows.rows[..before.unwrap_or(rows.rows.len())]
            .iter()
            .map(|row| row.key)
            .collect();
        let Some(hit) = search(rows, &line, cx)
            .into_iter()
            .rfind(|hit| keys.contains(&hit.key))
        else {
            return;
        };
        self.reveal(&hit, cx);
        self.flash = Some(Flash {
            hit,
            _fade: cx.spawn(async move |this, cx| {
                cx.background_executor().timer(FLASH).await;
                let _ = this.update(cx, |this, cx| {
                    this.flash = None;
                    this.apply_highlights(cx);
                });
            }),
        });
        self.apply_highlights(cx);
        cx.notify();
    }

    /// The bar at the top right of the conversation.
    pub(super) fn find_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        let now = Instant::now();
        let find = self.find.as_ref()?;
        let progress = find.presence.progress(now);
        let animating = find.presence.is_animating(now);
        motion::request_frame(window, animating);
        if progress <= 0. && !animating {
            self.find = None;
            return None;
        }
        let find = self.find.as_ref()?;
        let open = find.open;
        let empty = find.input.read(cx).value().is_empty();
        let remote = self.rt(cx).is_remote();
        let count = if remote {
            find.remote.len()
        } else {
            find.hits.len()
        };
        let counter = match if remote {
            find.remote_current
        } else {
            find.current
        } {
            Some(current) => format!("{} of {count}", current + 1),
            None if empty => String::new(),
            None if find.remote_loading => "Searching…".to_owned(),
            None => "No results".to_owned(),
        };
        let step = |id: &'static str, name: IconName, label: &'static str, delta: isize| {
            icon_button(id, name, label)
                .disabled(count == 0)
                .on_click(cx.listener(move |this, _, _, cx| this.find_step(delta, cx)))
        };
        Some(
            div()
                .absolute()
                .top(px(10. - 6. * (1. - progress)))
                .right(px(16.))
                .opacity(progress)
                .child(
                    row()
                        .id("chat-find")
                        .key_context("ChatFind")
                        .occlude()
                        .w(px(340.))
                        .p(px(4.))
                        .gap(px(2.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(t::control_border())
                        .bg(t::elevated())
                        .shadow_md()
                        .on_action(cx.listener(|this, _: &super::CloseFind, window, cx| {
                            this.close_find(window, cx);
                        }))
                        // Before the field, which keeps Esc for its own menus.
                        .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape"
                                && !event.keystroke.modifiers.modified()
                                && this.close_find(window, cx)
                            {
                                cx.stop_propagation();
                            }
                        }))
                        .child(
                            div().flex_1().min_w_0().child(
                                input(&find.input)
                                    .appearance(false)
                                    .disabled(!open)
                                    .prefix(icon(IconName::Search).text_color(t::muted())),
                            ),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .px(px(6.))
                                .text_size(px(12.))
                                .text_color(t::muted())
                                .child(counter),
                        )
                        .child(step(
                            "chat-find-previous",
                            IconName::ChevronUp,
                            "Previous match (⇧⏎)",
                            -1,
                        ))
                        .child(step(
                            "chat-find-next",
                            IconName::ChevronDown,
                            "Next match (⏎)",
                            1,
                        ))
                        .child(
                            icon_button("chat-find-close", IconName::Close, "Close (Esc)")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_find(window, cx);
                                })),
                        ),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn matches_ignore_case_and_stay_on_characters() {
        assert_eq!(
            matches("Find the fox, FOX and fOx", "fox"),
            vec![9..12, 14..17, 22..25]
        );
        assert_eq!(matches("Zażółć GĘŚLĄ jaźń", "gęślą"), vec![11..19]);
        assert_eq!(matches("aaaa", "aa"), vec![0..2, 2..4]);
        assert!(matches("anything", "").is_empty());
        assert!(matches("short", "longer than it").is_empty());
        // "İ" lowercases to two characters; half of it is no match.
        assert!(matches("İx", "x").len() == 1 && matches("İ", "i").is_empty());
    }
}
