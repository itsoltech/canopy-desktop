//! Text quoted from the conversation (DEC-070): a badge in the message as written and
//! a quote block where that badge is in the message the agent reads.
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

/// The message as the agent reads it: each quote's mention becomes a quote block there.
/// A quote whose mention isn't in the text comes before it.
pub fn expand(text: &str, attachments: &[Attachment]) -> String {
    let mut out = String::new();
    let mut from = 0;
    for attachment in attachments {
        let Attachment::Quote {
            text: quoted,
            label,
        } = attachment
        else {
            continue;
        };
        let found = label
            .as_deref()
            .and_then(|label| Some((text[from..].find(label)? + from, label.len())));
        match found {
            Some((at, len)) => {
                out.push_str(text[from..at].trim_end_matches([' ', '\t']));
                block(quoted, &mut out);
                from = at + len;
                // The space put after the badge.
                while text[from..].starts_with([' ', '\t']) {
                    from += 1;
                }
            }
            None if from == 0 => block(quoted, &mut out),
            None => {}
        }
    }
    out.push_str(&text[from..]);
    out.trim().to_owned()
}

/// `quoted` as a Markdown quote on lines of its own.
fn block(quoted: &str, out: &mut String) {
    let kept = out.trim_end().len();
    out.truncate(kept);
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    for line in quoted.lines() {
        out.push('>');
        if !line.is_empty() {
            out.push(' ');
            out.push_str(line);
        }
        out.push('\n');
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quoted(text: &str) -> (Attachment, String) {
        let quote = quote(text).expect("quote");
        let label = quote.label().unwrap().to_owned();
        (quote, label)
    }

    #[test]
    fn quotes_become_blocks_where_their_badges_are() {
        let (first, a) = quoted("Rows::insert creates groups\nwhen a reply arrives");
        let (second, b) = quoted("short");
        let text = format!("why {a} and not {b} here?");
        assert_eq!(
            expand(&text, &[first, second]),
            "why\n\n> Rows::insert creates groups\n> when a reply arrives\n\nand not\n\n> short\n\nhere?"
        );
    }

    #[test]
    fn a_message_of_only_a_quote_and_one_not_mentioned() {
        let (only, label) = quoted("all of it");
        assert_eq!(expand(&format!("{label} "), &[only]), "> all of it");
        let (unmentioned, _) = quoted("lost badge");
        assert_eq!(expand("hello", &[unmentioned]), "> lost badge\n\nhello");
        assert_eq!(expand("plain", &[]), "plain");
    }

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
