//! Fenced code blocks in a message (DEC-088): the composer shows them as code editors
//! between its text fields, the agent gets the markdown fences, and a message put back
//! into the composer is split into the same parts again.
use std::ops::Range;

/// A part of a message: text, or a code block with its fence's language tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    /// Byte range of the text in the message.
    Text(Range<usize>),
    Code {
        language: String,
        code: String,
    },
}

/// The message as text and code blocks, in order. A fence opens on a line starting
/// with ```` ``` ```` and closes on a line of only ```` ``` ````; one left open stays
/// text. Text before a block loses the line break before its fence, text after it
/// the one after the closing fence, as `join` adds them back.
pub fn split(text: &str) -> Vec<Piece> {
    let mut pieces = vec![];
    let mut text_start = 0;
    let mut at = 0;
    while at < text.len() {
        let line_end = text[at..].find('\n').map_or(text.len(), |n| at + n);
        let line = &text[at..line_end];
        if let Some(info) = line.strip_prefix("```")
            && !info.contains('`')
            && let Some((code, close_end)) = closing(text, line_end)
        {
            let before_end = at.saturating_sub(1).max(text_start);
            pieces.push(Piece::Text(text_start..before_end));
            pieces.push(Piece::Code {
                language: info.trim().to_owned(),
                code,
            });
            // The closing fence's line break belongs to the block.
            text_start = (close_end + 1).min(text.len());
            at = text_start;
            continue;
        }
        at = line_end + 1;
    }
    pieces.push(Piece::Text(text_start.min(text.len())..text.len()));
    pieces
}

/// The code after an opening fence ending at `open_end`, and where its closing
/// fence's line ends.
fn closing(text: &str, open_end: usize) -> Option<(String, usize)> {
    if open_end >= text.len() {
        return None;
    }
    let mut at = open_end + 1;
    let start = at;
    loop {
        let line_end = text[at..].find('\n').map_or(text.len(), |n| at + n);
        if text[at..line_end].trim_end() == "```" {
            let code = &text[start..at.saturating_sub(1).max(start)];
            return Some((code.to_owned(), line_end));
        }
        if line_end >= text.len() {
            return None;
        }
        at = line_end + 1;
    }
}

/// A part to put together into a message.
pub enum Part<'a> {
    Text(&'a str),
    Code { language: &'a str, code: &'a str },
}

/// The message markdown: text as written, each block on its own lines in fences.
pub fn join<'a>(parts: impl IntoIterator<Item = Part<'a>>) -> String {
    let mut out = String::new();
    let mut after_block = false;
    for part in parts {
        match part {
            Part::Text(text) => {
                if after_block && !text.is_empty() {
                    out.push('\n');
                }
                out.push_str(text);
                after_block = false;
            }
            Part::Code { language, code } => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```");
                out.push_str(language.trim());
                out.push('\n');
                out.push_str(code);
                out.push_str("\n```");
                after_block = true;
            }
        }
    }
    out
}

/// A language the composer's block offers: its fence tag and name.
pub const LANGUAGES: [(&str, &str); 13] = [
    ("", "Plain text"),
    ("bash", "Bash"),
    ("css", "CSS"),
    ("html", "HTML"),
    ("javascript", "JavaScript"),
    ("json", "JSON"),
    ("markdown", "Markdown"),
    ("python", "Python"),
    ("rust", "Rust"),
    ("svelte", "Svelte"),
    ("toml", "TOML"),
    ("typescript", "TypeScript"),
    ("yaml", "YAML"),
];

/// The highlighting grammar for a fence tag; `None` keeps the block plain. Agents and
/// people tag fences loosely (`Rust`, `TS`, `jsx`, `zsh`).
pub fn grammar(tag: &str) -> Option<&'static str> {
    // Info strings like `rust,ignore` or `{.python}` carry the language first.
    let tag = tag
        .trim()
        .trim_start_matches(['{', '.'])
        .split([',', ' ', '}'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    Some(match tag.as_str() {
        "rust" | "rs" => "rust",
        // The bundled TSX grammar barely captures anything; TypeScript recovers from
        // JSX well, and the JavaScript grammar parses JSX natively.
        "typescript" | "ts" | "mts" | "cts" | "tsx" => "typescript",
        "javascript" | "js" | "mjs" | "cjs" | "node" | "jsx" => "javascript",
        "python" | "py" | "python3" | "py3" => "python",
        "bash" | "sh" | "shell" | "zsh" | "console" | "shellscript" | "shell-session"
        | "terminal" => "bash",
        "html" | "htm" => "html",
        "css" | "scss" => "css",
        "json" | "jsonc" | "json5" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "markdown" | "md" | "mdx" => "markdown",
        "svelte" => "svelte",
        _ => return None,
    })
}

/// What the block's language chip says for a fence tag.
pub fn language_name(tag: &str) -> String {
    match grammar(tag) {
        Some(grammar) => LANGUAGES
            .iter()
            .find(|(tag, _)| *tag == grammar)
            .map_or(grammar, |(_, name)| name)
            .to_owned(),
        None if tag.trim().is_empty() => "Plain text".to_owned(),
        None => tag.trim().to_owned(),
    }
}

/// The fence a line typed in the composer opens: ```` ``` ```` with an optional
/// language tag and nothing else.
pub fn opening(line: &str) -> Option<&str> {
    let tag = line.trim_end().strip_prefix("```")?.trim();
    (!tag.contains(char::is_whitespace) && !tag.contains('`')).then_some(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(text: &str, pieces: &[Piece]) -> Vec<String> {
        pieces
            .iter()
            .map(|piece| match piece {
                Piece::Text(range) => format!("T:{}", &text[range.clone()]),
                Piece::Code { language, code } => format!("C:{language}:{code}"),
            })
            .collect()
    }

    #[test]
    fn blocks_split_out_and_join_back() {
        let text = "Fix this:\n```rust\nfn main() {\n    run();\n}\n```\nand add a test";
        let pieces = split(text);
        assert_eq!(
            texts(text, &pieces),
            [
                "T:Fix this:",
                "C:rust:fn main() {\n    run();\n}",
                "T:and add a test"
            ]
        );
        let parts = pieces.iter().map(|piece| match piece {
            Piece::Text(range) => Part::Text(&text[range.clone()]),
            Piece::Code { language, code } => Part::Code { language, code },
        });
        assert_eq!(join(parts), text);
    }

    #[test]
    fn edges_and_unclosed_fences() {
        // A block first and last, an empty one, and one never closed.
        let text = "```\n\n```";
        assert_eq!(texts(text, &split(text)), ["T:", "C::", "T:"]);
        let text = "```py\nx = 1\n```";
        assert_eq!(texts(text, &split(text)), ["T:", "C:py:x = 1", "T:"]);
        let text = "see ```inline``` and\n```sh\necho";
        assert_eq!(texts(text, &split(text)), [format!("T:{text}")]);
        assert_eq!(
            join([
                Part::Code {
                    language: "",
                    code: "a"
                },
                Part::Text("after"),
            ]),
            "```\na\n```\nafter"
        );
    }

    #[test]
    fn blocks_next_to_each_other_stay_so() {
        let text = join([
            Part::Text("a"),
            Part::Code {
                language: "",
                code: "x",
            },
            Part::Text(""),
            Part::Code {
                language: "py",
                code: "y",
            },
            Part::Text(""),
        ]);
        assert_eq!(text, "a\n```\nx\n```\n```py\ny\n```");
        assert_eq!(
            texts(&text, &split(&text)),
            ["T:a", "C::x", "T:", "C:py:y", "T:"]
        );
    }

    #[test]
    fn typed_fences_and_language_names() {
        assert_eq!(opening("```"), Some(""));
        assert_eq!(opening("```ts  "), Some("ts"));
        assert_eq!(opening("``` rust"), Some("rust"));
        assert_eq!(opening("text ```"), None);
        assert_eq!(opening("```a b"), None);
        assert_eq!(language_name("ts"), "TypeScript");
        assert_eq!(language_name(""), "Plain text");
        assert_eq!(language_name("go"), "go");
    }
}
