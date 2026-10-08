//! Preparation policy for remote Markdown rendered by native GPUI TextView.
use gpui_kit::http_client::Url;
use markdown_parser::{ParseOptions, mdast::Node};
use std::collections::HashMap;

/// Relative links use the issue URL. Only browser web URLs can be opened.
pub fn link_target(target: &str, base: &str) -> Option<String> {
    let url = Url::parse(target.trim())
        .ok()
        .or_else(|| Url::parse(base).ok()?.join(target.trim()).ok())?;
    (matches!(url.scheme(), "https" | "http")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.to_string())
}

fn literal(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii_punctuation() {
            result.push('\\');
        }
        result.push(c);
    }
    result
}
fn image_link(alt: &str, target: &str, base: &str) -> String {
    let label = if alt.trim().is_empty() {
        "View image".to_owned()
    } else {
        format!("Image: {}", alt.replace(['\n', '\r'], " "))
    };
    match (!target.trim().is_empty())
        .then(|| link_target(target, base))
        .flatten()
    {
        Some(url) => format!(
            "[{}]({})",
            literal(&label),
            url.replace('(', "%28").replace(')', "%29")
        ),
        None => literal(&label),
    }
}
fn definitions<'a>(node: &'a Node, result: &mut HashMap<&'a str, &'a str>) {
    if let Node::Definition(definition) = node {
        result
            .entry(&definition.identifier)
            .or_insert(&definition.url);
    }
    if let Some(children) = node.children() {
        for child in children {
            definitions(child, result);
        }
    }
}
fn replacements(
    node: &Node,
    definitions: &HashMap<&str, &str>,
    base: &str,
    result: &mut Vec<(std::ops::Range<usize>, String)>,
) {
    let replacement = match node {
        Node::Image(image) => Some(image_link(&image.alt, &image.url, base)),
        Node::ImageReference(image) => Some(image_link(
            &image.alt,
            definitions
                .get(image.identifier.as_str())
                .copied()
                .unwrap_or(""),
            base,
        )),
        // Raw HTML is displayed as text, never passed to the renderer's image
        // or HTML loader. Markdown code spans/fences are separate AST nodes.
        Node::Html(html) => Some(if html.value.starts_with("<!--") {
            String::new()
        } else if matches!(
            html.value.to_ascii_lowercase().as_str(),
            "<br>" | "<br/>" | "<br />"
        ) {
            "  \n".into()
        } else {
            literal(&html.value)
        }),
        _ => None,
    };
    if let (Some(value), Some(position)) = (replacement, node.position()) {
        result.push((position.start.offset..position.end.offset, value));
    } else if let Some(children) = node.children() {
        for child in children {
            replacements(child, definitions, base, result);
        }
    }
}

/// Preserve GFM formatting while keeping images click-to-open and raw HTML inert.
/// Offsets come from the parser, so code examples and escaped image syntax survive.
pub fn prepare(source: &str, base: &str) -> String {
    let Ok(tree) = markdown_parser::to_mdast(source, &ParseOptions::gfm()) else {
        return literal(source);
    };
    let mut links = HashMap::new();
    definitions(&tree, &mut links);
    let mut edits = Vec::new();
    replacements(&tree, &links, base, &mut edits);
    let mut output = source.to_owned();
    for (range, value) in edits.into_iter().rev() {
        output.replace_range(range, &value);
    }
    output
}

/// A reply split at its top-level code blocks, so each block can get its own view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    Prose(String),
    Code { lang: Option<String>, code: String },
}

/// Prose and top-level code blocks in source order. Code nested in lists or quotes
/// stays inside its prose. An unclosed fence (still streaming) runs to the end.
pub fn split_code(source: &str) -> Vec<Segment> {
    let Ok(tree) = markdown_parser::to_mdast(source, &ParseOptions::gfm()) else {
        return vec![Segment::Prose(source.to_owned())];
    };
    let mut segments = vec![];
    let mut last = 0;
    for child in tree.children().into_iter().flatten() {
        let (Node::Code(code), Some(position)) = (child, child.position()) else {
            continue;
        };
        let start = position.start.offset;
        if let Some(prose) = source.get(last..start).filter(|p| !p.trim().is_empty()) {
            segments.push(Segment::Prose(prose.to_owned()));
        }
        segments.push(Segment::Code {
            lang: code.lang.clone(),
            code: code.value.clone(),
        });
        last = position.end.offset;
    }
    match source.get(last..) {
        Some(rest) if !rest.trim().is_empty() || segments.is_empty() => {
            segments.push(Segment::Prose(rest.to_owned()))
        }
        _ => {}
    }
    segments
}

#[cfg(test)]
mod split_tests {
    use super::*;

    #[test]
    fn top_level_code_blocks_become_their_own_segments() {
        let source = "Intro\n\n```rust\nfn main() {}\n```\n\n- item\n  ```\n  nested\n  ```\n\nEnd";
        let segments = split_code(source);
        assert_eq!(segments.len(), 3, "{segments:?}");
        assert_eq!(segments[0], Segment::Prose("Intro\n\n".into()));
        assert_eq!(
            segments[1],
            Segment::Code {
                lang: Some("rust".into()),
                code: "fn main() {}".into()
            }
        );
        assert!(
            matches!(&segments[2], Segment::Prose(p) if p.contains("nested") && p.ends_with("End"))
        );
    }

    #[test]
    fn a_streaming_fence_runs_to_the_end() {
        let segments = split_code("Look:\n\n```py\nprint(1)\nprint(");
        assert_eq!(
            segments[1],
            Segment::Code {
                lang: Some("py".into()),
                code: "print(1)\nprint(".into()
            }
        );
        assert_eq!(split_code(""), [Segment::Prose(String::new())]);
    }
}

/// Monospace columns of a code line: tabs to the next multiple of 4, wide (CJK,
/// emoji) characters as two. An estimate for scroll widths, not a layout.
pub fn columns(line: &str) -> usize {
    let mut columns = 0;
    for c in line.chars() {
        columns += match c {
            '\t' => 4 - columns % 4,
            '\u{1100}'..='\u{115F}'
            | '\u{2E80}'..='\u{A4CF}'
            | '\u{AC00}'..='\u{D7A3}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{FE30}'..='\u{FE4F}'
            | '\u{FF00}'..='\u{FF60}'
            | '\u{FFE0}'..='\u{FFE6}'
            | '\u{1F300}'..='\u{1FAFF}'
            | '\u{20000}'..='\u{3FFFD}' => 2,
            _ => 1,
        };
    }
    columns
}

#[cfg(test)]
mod column_tests {
    use super::columns;

    #[test]
    fn tabs_and_wide_characters_take_more_columns() {
        assert_eq!(columns("abc"), 3);
        assert_eq!(columns("\tx"), 5);
        assert_eq!(columns("ab\tx"), 5);
        assert_eq!(columns("日本"), 4);
        assert_eq!(columns("ok 🚀"), 5);
    }
}
