//! Links in terminal output: web addresses in the text and OSC 8 hyperlinks set
//! by programs (agent CLIs). Only http(s) opens (Electron: `openTerminalUrl`).
use super::session::Frame;
use alacritty_terminal::term::cell::Flags;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub url: String,
    /// Visible cells of the link per row: (row, first column, last column).
    pub spans: Vec<(usize, usize, usize)>,
}

impl Link {
    pub fn contains(&self, row: usize, column: usize) -> bool {
        self.spans
            .iter()
            .any(|&(r, first, last)| r == row && (first..=last).contains(&column))
    }
}

/// The address to hand to the browser: only an http(s) URL, normalized.
pub fn openable(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map(String::from)
}

/// Groups cells given in reading order into per-row spans.
pub(crate) fn spans(cells: impl IntoIterator<Item = (usize, usize)>) -> Vec<(usize, usize, usize)> {
    let mut spans: Vec<(usize, usize, usize)> = Vec::new();
    for (row, column) in cells {
        match spans.last_mut() {
            Some(span) if span.0 == row => span.2 = column,
            _ => spans.push((row, column, column)),
        }
    }
    spans
}

/// The web address under a visible cell. A line the terminal wrapped is read
/// as one, so an address broken across rows stays one link.
pub fn url_at(frame: &Frame, row: usize, column: usize) -> Option<Link> {
    let columns = frame.size.columns;
    if columns == 0 || row >= frame.size.rows {
        return None;
    }
    let wraps = |row: usize| {
        frame
            .cells
            .get(row * columns + columns - 1)
            .is_some_and(|cell| cell.flags.contains(Flags::WRAPLINE))
    };
    let mut first = row;
    while first > 0 && wraps(first - 1) {
        first -= 1;
    }
    let mut last = row;
    while last + 1 < frame.size.rows && wraps(last) {
        last += 1;
    }
    // Text of the line and, per cell, where its characters start.
    let mut text = String::new();
    let mut starts = Vec::new();
    for cell in frame.cells[first * columns..((last + 1) * columns).min(frame.cells.len())]
        .iter()
        .filter(|cell| {
            !cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        })
    {
        starts.push((text.len(), cell));
        text.push_str(&cell.text);
    }
    find_urls(&text).into_iter().find_map(|range| {
        let cells = starts
            .iter()
            .filter(|(start, _)| range.contains(start))
            .flat_map(|(_, cell)| {
                [
                    (cell.row, cell.column),
                    (cell.row, cell.column + cell.width.max(1) - 1),
                ]
            });
        let link = Link {
            url: text[range.clone()].to_owned(),
            spans: spans(cells),
        };
        link.contains(row, column).then_some(link)
    })
}

/// Byte ranges of http(s) addresses, without punctuation that ends a sentence
/// or closes brackets opened before the address.
fn find_urls(text: &str) -> Vec<Range<usize>> {
    let mut urls = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find("http") {
        let start = from + found;
        let rest = &text[start..];
        let scheme = if rest.starts_with("https://") {
            8
        } else if rest.starts_with("http://") {
            7
        } else {
            from = start + 4;
            continue;
        };
        let boundary = text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let mut end = start
            + rest
                .find(|c: char| {
                    c.is_whitespace()
                        || c.is_control()
                        || matches!(
                            c,
                            '<' | '>' | '"' | '\'' | '`' | '{' | '}' | '|' | '\\' | '^'
                        )
                })
                .unwrap_or(rest.len());
        while let Some(last) = text[start..end].chars().next_back() {
            let url = &text[start..end];
            let unbalanced = |open: char, close: char| {
                last == close && url.matches(open).count() < url.matches(close).count()
            };
            if matches!(last, '.' | ',' | ';' | ':' | '!' | '?')
                || unbalanced('(', ')')
                || unbalanced('[', ']')
            {
                end -= last.len_utf8();
            } else {
                break;
            }
        }
        if boundary && end > start + scheme {
            urls.push(start..end);
        }
        from = end.max(start + scheme);
    }
    urls
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::session::{Cell, Size};
    use alacritty_terminal::vte::ansi::{CursorShape, Rgb};

    fn frame(rows: &[&str], columns: usize, wrapped: &[usize]) -> Frame {
        let black = Rgb { r: 0, g: 0, b: 0 };
        let cells = rows
            .iter()
            .enumerate()
            .flat_map(|(row, text)| {
                let chars: Vec<char> = text.chars().collect();
                (0..columns).map(move |column| Cell {
                    row,
                    column,
                    text: chars.get(column).unwrap_or(&' ').to_string(),
                    width: 1,
                    fg: black,
                    bg: black,
                    flags: if column == columns - 1 && wrapped.contains(&row) {
                        Flags::WRAPLINE
                    } else {
                        Flags::empty()
                    },
                    selected: false,
                })
            })
            .collect();
        Frame {
            cells,
            cursor: (0, 0),
            cursor_shape: CursorShape::Block,
            mode: Default::default(),
            offset: 0,
            size: Size {
                columns,
                rows: rows.len(),
            },
            revision: 0,
        }
    }

    #[test]
    fn an_address_wrapped_across_rows_is_one_link_without_trailing_punctuation() {
        let frame = frame(&["Local: http://localh", "ost:5173/app). Done"], 20, &[0]);
        let link = url_at(&frame, 1, 3).unwrap();
        assert_eq!(link.url, "http://localhost:5173/app");
        assert_eq!(link.spans, vec![(0, 7, 19), (1, 0, 11)]);
        assert_eq!(url_at(&frame, 0, 2), None);
        assert_eq!(url_at(&frame, 1, 13), None);
    }

    #[test]
    fn balanced_brackets_stay_and_only_web_addresses_open() {
        let frame = frame(
            &["see (https://en.wikipedia.org/wiki/Rust_(language))"],
            52,
            &[],
        );
        assert_eq!(
            url_at(&frame, 0, 10).unwrap().url,
            "https://en.wikipedia.org/wiki/Rust_(language)"
        );
        assert_eq!(
            openable("https://example.com/a b").as_deref(),
            Some("https://example.com/a%20b")
        );
        assert_eq!(openable("file:///Applications/Calculator.app"), None);
        assert_eq!(openable("javascript:alert(1)"), None);
    }
}
