//! ADF stays in task snapshots. Unedited rich fields are never round-tripped through Markdown.
use markdown_parser::{ParseOptions, mdast::Node};
use serde_json::{Value, json};
pub fn editable(v: &Value) -> bool {
    fn walk(v: &Value, depth: usize) -> bool {
        if depth > 64 {
            return false;
        }
        let kind = v["type"].as_str().unwrap_or("");
        if !matches!(
            kind,
            "doc"
                | "paragraph"
                | "text"
                | "heading"
                | "bulletList"
                | "orderedList"
                | "listItem"
                | "blockquote"
                | "codeBlock"
                | "hardBreak"
                | "rule"
        ) {
            return false;
        }
        if let Some(marks) = v["marks"].as_array()
            && marks.iter().any(|m| {
                !matches!(
                    m["type"].as_str(),
                    Some("strong" | "em" | "strike" | "code" | "link")
                )
            })
        {
            return false;
        }
        if v.get("attrs")
            .is_some_and(|a| a.is_object() && !a.as_object().unwrap().is_empty())
            && !matches!(kind, "heading" | "orderedList" | "codeBlock")
        {
            return false;
        }
        v["content"]
            .as_array()
            .is_none_or(|items| items.iter().all(|n| walk(n, depth + 1)))
    }
    v.is_null() || v.is_string() || walk(v, 0)
}
fn literal(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            if matches!(
                c,
                '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|' | '~'
            ) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}
pub fn to_markdown(value: &Value) -> String {
    fn walk(v: &Value, depth: usize) -> String {
        if depth > 64 {
            return "[Nested Jira content]".into();
        }
        if let Some(text) = v.as_str() {
            return text.into();
        }
        let children = || {
            v["content"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|n| walk(n, depth + 1))
                .collect::<Vec<_>>()
        };
        let content = || children().join("");
        match v["type"].as_str().unwrap_or("") {
            "text" => {
                let raw = v["text"].as_str().unwrap_or("");
                let mut text = literal(raw);
                for mark in v["marks"].as_array().into_iter().flatten() {
                    text = match mark["type"].as_str().unwrap_or("") {
                        "strong" => format!("**{text}**"),
                        "em" => format!("*{text}*"),
                        "strike" => format!("~~{text}~~"),
                        "code" => {
                            let fence = "`".repeat(
                                raw.split(|c| c != '`')
                                    .map(str::len)
                                    .max()
                                    .unwrap_or(0)
                                    .max(1)
                                    + 1,
                            );
                            format!("{fence} {raw} {fence}")
                        }
                        "link" => format!(
                            "[{text}](<{}>)",
                            mark["attrs"]["href"]
                                .as_str()
                                .unwrap_or("")
                                .replace('>', "%3E")
                        ),
                        _ => text,
                    };
                }
                text
            }
            "paragraph" => format!("{}\n\n", content()),
            "heading" => format!(
                "{} {}\n\n",
                "#".repeat(v["attrs"]["level"].as_u64().unwrap_or(2).clamp(1, 6) as usize),
                content()
            ),
            "hardBreak" => "  \n".into(),
            "rule" => "\n---\n\n".into(),
            "codeBlock" => {
                let text = v["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|n| n["text"].as_str())
                    .collect::<String>();
                let fence = "`".repeat(
                    text.split(|c| c != '`')
                        .map(str::len)
                        .max()
                        .unwrap_or(0)
                        .max(2)
                        + 1,
                );
                format!(
                    "{fence}{}\n{text}\n{fence}\n\n",
                    v["attrs"]["language"].as_str().unwrap_or("")
                )
            }
            "bulletList" | "orderedList" => {
                children()
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let prefix = if v["type"] == "orderedList" {
                            format!(
                                "{}. ",
                                i + v["attrs"]["order"].as_u64().unwrap_or(1) as usize
                            )
                        } else {
                            "- ".into()
                        };
                        format!(
                            "{prefix}{}\n",
                            item.trim()
                                .replace('\n', &format!("\n{}", " ".repeat(prefix.len())))
                        )
                    })
                    .collect::<String>()
                    + "\n"
            }
            "blockquote" | "panel" => {
                content()
                    .trim()
                    .lines()
                    .map(|l| format!("> {l}\n"))
                    .collect::<String>()
                    + "\n"
            }
            "table" => {
                let rows = v["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|r| {
                        r["content"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|c| {
                                walk(c, depth + 1)
                                    .trim()
                                    .replace('\n', "<br>")
                                    .replace('|', "\\|")
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                let mut out = String::new();
                for (i, row) in rows.iter().enumerate() {
                    out += &format!("| {} |\n", row.join(" | "));
                    if i == 0 {
                        out += &format!("| {} |\n", vec!["---"; row.len()].join(" | "));
                    }
                }
                out + "\n"
            }
            "mention" => literal(v["attrs"]["text"].as_str().unwrap_or("@user")),
            "emoji" => v["attrs"]["text"]
                .as_str()
                .or(v["attrs"]["shortName"].as_str())
                .unwrap_or("")
                .into(),
            "inlineCard" | "blockCard" => {
                let u = v["attrs"]["url"].as_str().unwrap_or("");
                format!("[{}](<{u}>)", literal(u))
            }
            "media" | "mediaSingle" | "mediaGroup" => {
                "\n[Attachment — see attached files below]\n\n".into()
            }
            "date" => v["attrs"]["timestamp"].as_str().unwrap_or("").into(),
            "status" => format!("**{}**", literal(v["attrs"]["text"].as_str().unwrap_or(""))),
            _ => content(),
        }
    }
    walk(value, 0).trim().into()
}
pub fn from_markdown(text: &str) -> Result<Value, String> {
    let mut tree = markdown_parser::to_mdast(text, &ParseOptions::gfm())
        .map_err(|_| "Could not parse this Markdown.")?;
    fn definitions(
        node: &Node,
        links: &mut std::collections::HashMap<String, (String, Option<String>)>,
    ) {
        if let Node::Definition(d) = node {
            links
                .entry(d.identifier.clone())
                .or_insert((d.url.clone(), d.title.clone()));
        }
        if let Some(children) = node.children() {
            for child in children {
                definitions(child, links);
            }
        }
    }
    fn expand(
        node: &mut Node,
        links: &std::collections::HashMap<String, (String, Option<String>)>,
    ) {
        match node {
            Node::LinkReference(link) => {
                if let Some((url, title)) = links.get(&link.identifier) {
                    *node = Node::Link(markdown_parser::mdast::Link {
                        children: link.children.clone(),
                        position: link.position.clone(),
                        url: url.clone(),
                        title: title.clone(),
                    });
                }
            }
            Node::ImageReference(image) => {
                if let Some((url, title)) = links.get(&image.identifier) {
                    *node = Node::Image(markdown_parser::mdast::Image {
                        alt: image.alt.clone(),
                        position: image.position.clone(),
                        url: url.clone(),
                        title: title.clone(),
                    });
                }
            }
            _ => {}
        }
        if let Some(children) = node.children_mut() {
            children.retain(|n| !matches!(n, Node::Definition(_)));
            for child in children {
                expand(child, links);
            }
        }
    }
    let mut links = std::collections::HashMap::new();
    definitions(&tree, &mut links);
    expand(&mut tree, &links);
    fn inline(node: &Node, marks: Vec<Value>) -> Vec<Value> {
        match node {
            Node::Text(t) => vec![json!({"type":"text","text":t.value,"marks":marks})],
            Node::InlineCode(c) => {
                let mut m = marks;
                m.push(json!({"type":"code"}));
                vec![json!({"type":"text","text":c.value,"marks":m})]
            }
            Node::Break(_) => vec![json!({"type":"hardBreak"})],
            Node::Strong(_) | Node::Emphasis(_) | Node::Delete(_) | Node::Link(_) => {
                let mut m = marks;
                m.push(match node {
                    Node::Strong(_) => json!({"type":"strong"}),
                    Node::Emphasis(_) => json!({"type":"em"}),
                    Node::Delete(_) => json!({"type":"strike"}),
                    Node::Link(l) => json!({"type":"link","attrs":{"href":l.url}}),
                    _ => unreachable!(),
                });
                node.children()
                    .into_iter()
                    .flatten()
                    .flat_map(|c| inline(c, m.clone()))
                    .collect()
            }
            Node::Image(i) => vec![
                json!({"type":"text","text":if i.alt.is_empty(){i.url.clone()}else{i.alt.clone()},"marks":[{"type":"link","attrs":{"href":i.url}}]}),
            ],
            _ => vec![json!({"type":"text","text":node.to_string(),"marks":marks})],
        }
    }
    fn block(node: &Node) -> Value {
        let blocks = || {
            node.children()
                .into_iter()
                .flatten()
                .map(block)
                .collect::<Vec<_>>()
        };
        let inlines = || {
            node.children()
                .into_iter()
                .flatten()
                .flat_map(|c| inline(c, vec![]))
                .filter(|v| {
                    v["type"] != "text" || v["text"].as_str().is_some_and(|s| !s.is_empty())
                })
                .collect::<Vec<_>>()
        };
        match node {
            Node::Root(_) => json!({"type":"doc","version":1,"content":blocks()}),
            Node::Heading(h) => {
                json!({"type":"heading","attrs":{"level":h.depth},"content":inlines()})
            }
            Node::Paragraph(_) => json!({"type":"paragraph","content":inlines()}),
            Node::List(l) => {
                if l.ordered {
                    json!({"type":"orderedList","attrs":{"order":l.start.unwrap_or(1)},"content":blocks()})
                } else {
                    json!({"type":"bulletList","content":blocks()})
                }
            }
            Node::ListItem(item) => {
                let mut content = blocks();
                if let Some(checked) = item.checked {
                    let prefix = json!({"type":"text","text":if checked {"[x] "}else{"[ ] "}});
                    if content.first().is_some_and(|v| v["type"] == "paragraph") {
                        content[0]["content"]
                            .as_array_mut()
                            .unwrap()
                            .insert(0, prefix);
                    } else {
                        content.insert(0, json!({"type":"paragraph","content":[prefix]}));
                    }
                }
                json!({"type":"listItem","content":content})
            }
            Node::Blockquote(_) => json!({"type":"blockquote","content":blocks()}),
            Node::Code(c) => {
                json!({"type":"codeBlock","attrs":{"language":c.lang.clone().unwrap_or_default()},"content":if c.value.is_empty(){vec![]}else{vec![json!({"type":"text","text":c.value})]}})
            }
            Node::ThematicBreak(_) => json!({"type":"rule"}),
            Node::Table(_) => json!({"type":"table","content":blocks()}),
            Node::TableRow(_) => json!({"type":"tableRow","content":blocks()}),
            Node::TableCell(_) => {
                json!({"type":"tableCell","content":[{"type":"paragraph","content":inlines()}]})
            }
            _ => json!({"type":"paragraph","content":[{"type":"text","text":node.to_string()}]}),
        }
    }
    let mut doc = block(&tree);
    if doc["content"].as_array().is_some_and(Vec::is_empty) {
        doc["content"] = json!([{"type":"paragraph","content":[]}]);
    }
    Ok(doc)
}

/// Preserve non-Markdown nodes as labeled, local references while text around them is edited.
/// The reference map belongs to this original document and never resolves network URLs.
fn protected_document(original: &Value) -> (Value, Vec<(Value, bool)>) {
    fn visit(v: &Value, block: bool, refs: &mut Vec<(Value, bool)>, depth: usize) -> Value {
        let mut shallow = v.clone();
        if shallow.get("content").is_some() {
            shallow["content"] = json!([]);
        }
        if depth > 64 || !editable(&shallow) {
            let id = refs.len();
            refs.push((v.clone(), block));
            let label = match v["type"].as_str().unwrap_or("content") {
                "mention" => v["attrs"]["text"].as_str().unwrap_or("Jira mention").into(),
                "table" => "Jira table".into(),
                "media" | "mediaSingle" | "mediaGroup" => "Jira attachment".into(),
                _ => {
                    let text = to_markdown(v);
                    if text.trim().is_empty() {
                        "Jira content".into()
                    } else {
                        text.lines()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(60)
                            .collect::<String>()
                    }
                }
            };
            let marker = json!({"type":"text","text":label,"marks":[{"type":"link","attrs":{"href":format!("canopy-jira://node/{id}")}}]});
            return if block {
                json!({"type":"paragraph","content":[marker]})
            } else {
                marker
            };
        }
        let mut result = v.clone();
        if let Some(children) = v["content"].as_array() {
            let child_block = !matches!(
                v["type"].as_str(),
                Some("paragraph" | "heading" | "codeBlock")
            );
            result["content"] = Value::Array(
                children
                    .iter()
                    .map(|c| visit(c, child_block, refs, depth + 1))
                    .collect(),
            );
        }
        result
    }
    let mut refs = vec![];
    let result = visit(original, true, &mut refs, 0);
    (result, refs)
}
pub fn editing_text(original: &Value) -> String {
    let (view, _) = protected_document(original);
    to_markdown(&view)
}
pub fn from_editing_text(text: &str, original: &Value) -> Result<Value, String> {
    let (_, refs) = protected_document(original);
    let parsed = from_markdown(text)?;
    fn reference<'a>(
        v: &Value,
        refs: &'a [(Value, bool)],
    ) -> Result<Option<&'a (Value, bool)>, String> {
        let href = v["marks"].as_array().into_iter().flatten().find_map(|m| {
            if m["type"] == "link" {
                m["attrs"]["href"].as_str()
            } else {
                None
            }
        });
        if let Some(href) = href
            && href.starts_with("canopy-jira:")
        {
            let id = href
                .strip_prefix("canopy-jira://node/")
                .and_then(|n| n.parse::<usize>().ok())
                .ok_or(
                    "The Jira content reference was changed. Keep its link unchanged or remove it.",
                )?;
            return refs
                .get(id)
                .map(Some)
                .ok_or("This Jira content reference no longer exists.".into());
        }
        Ok(None)
    }
    fn restore(v: Value, refs: &[(Value, bool)]) -> Result<Value, String> {
        if v["type"] == "paragraph"
            && let Some(children) = v["content"].as_array()
            && children.len() == 1
            && let Some((raw, true)) = reference(&children[0], refs)?
        {
            return Ok(raw.clone());
        }
        if let Some((raw, block)) = reference(&v, refs)? {
            if *block {
                return Err("Keep the Jira content link on its own paragraph.".into());
            }
            return Ok(raw.clone());
        }
        let mut v = v;
        if let Some(children) = v["content"].as_array() {
            v["content"] = Value::Array(
                children
                    .iter()
                    .cloned()
                    .map(|c| restore(c, refs))
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        Ok(v)
    }
    restore(parsed, &refs)
}
