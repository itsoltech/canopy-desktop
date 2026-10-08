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
