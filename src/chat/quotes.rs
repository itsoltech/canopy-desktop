//! Text quoted from the conversation (DEC-070): a badge in the message as written and
//! a quote block where that badge is in the message the agent reads (`compose`).
use super::transcript::Attachment;

/// Longest quote kept, in characters; longer ones end with "…".
pub const LIMIT: usize = 4_000;
/// Characters of the quote shown on its badge.
const EXCERPT: usize = 40;

/// `text` as a quote to attach, with its mention; `None` when there is nothing to quote.
pub fn quote(text: &str) -> Option<Attachment> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let text = match text.char_indices().nth(LIMIT) {
        Some((cut, _)) => format!("{}…", text[..cut].trim_end()),
        None => text.to_owned(),
    };
    Some(Attachment::Quote {
        label: Some(mention(&text)),
        text,
    })
}

/// `[❝ Rows::insert creates…]`: the quote's first words, on one line, as its mention.
pub fn mention(text: &str) -> String {
    let line = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(['[', ']'], "");
    let mut excerpt: String = line.chars().take(EXCERPT).collect();
    if line.chars().count() > EXCERPT {
        excerpt = format!("{}…", excerpt.trim_end());
    }
    format!("[❝ {excerpt}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_are_saved_with_their_message() {
        let quote = quote("saved").unwrap();
        let json = serde_json::to_value(&quote).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"Quote": {"text": "saved", "label": "[❝ saved]"}})
        );
        assert_eq!(serde_json::from_value::<Attachment>(json).unwrap(), quote);
    }

    #[test]
    fn long_quotes_are_cut_and_mentions_stay_on_one_line() {
        assert!(quote("  \n ").is_none());
        let Attachment::Quote { text, label } = quote(&"word ".repeat(2_000)).unwrap() else {
            unreachable!()
        };
        assert!(text.ends_with('…'));
        assert!(text.chars().count() <= LIMIT + 1);
        let label = label.unwrap();
        assert!(label.starts_with("[❝ word word") && label.ends_with("…]"));
        assert_eq!(mention("a [b]\n  c"), "[❝ a b c]");
    }
}
