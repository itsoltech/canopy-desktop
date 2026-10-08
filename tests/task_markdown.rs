use canopy_desktop::markdown::{link_target, prepare};
use markdown_parser::{ParseOptions, mdast::Node};
const BASE: &str = "https://github.com/acme/canopy/issues/12";
fn assert_no_embeds(node: &Node) {
    assert!(!matches!(
        node,
        Node::Image(_) | Node::ImageReference(_) | Node::Html(_)
    ));
    if let Some(children) = node.children() {
        for child in children {
            assert_no_embeds(child);
        }
    }
}
#[test]
fn preserves_gfm_lists_tables_code_and_unicode() {
    let source = "## Zażółć\n\n**Bold** and _italic_, ~~old~~ and `code`.\n\n- [x] Done\n- [ ] Next\n\n> Quote\n\n| A | B |\n| - | - |\n| 1 | 2 |\n\n```html\n<img src=\"file:///example\">\n![literal](https://example.com/image.png)\n```\n";
    assert_eq!(prepare(source, BASE), source);
}
#[test]
fn images_become_links_and_html_cannot_load_local_or_remote_content() {
    let source = "Żółć ![screen](https://example.com/image(a).png) and ![ref][shot].\n\n[shot]: /attachments/screen.png\n\n<img src=\"file:///private/file\">\n\n![bad](file:///private/file)\n\n<!-- comment -->\n";
    let prepared = prepare(source, BASE);
    let tree = markdown_parser::to_mdast(&prepared, &ParseOptions::gfm()).unwrap();
    assert_no_embeds(&tree);
    fn links(node: &Node, urls: &mut Vec<String>) {
        if let Node::Link(link) = node {
            urls.push(link.url.clone());
        }
        if let Some(children) = node.children() {
            for child in children {
                links(child, urls);
            }
        }
    }
    let mut urls = Vec::new();
    links(&tree, &mut urls);
    assert_eq!(
        urls,
        vec![
            "https://example.com/image%28a%29.png",
            "https://github.com/attachments/screen.png"
        ]
    );
    assert!(prepared.contains("Żółć"));
}
#[test]
fn supports_browser_links_but_not_app_commands_or_local_files() {
    assert_eq!(
        link_target("/acme/canopy/issues/5", BASE).as_deref(),
        Some("https://github.com/acme/canopy/issues/5")
    );
    assert_eq!(
        link_target("https://example.com/docs", BASE).as_deref(),
        Some("https://example.com/docs")
    );
    for target in [
        "javascript:alert(1)",
        "file:///tmp/file",
        "data:text/html,test",
        "vscode://file/tmp/a",
        "https://user:secret@example.com/",
    ] {
        assert!(link_target(target, BASE).is_none(), "{target}");
    }
}

#[test]
fn agent_markdown_without_a_web_base_opens_only_explicit_web_links() {
    assert_eq!(
        canopy_desktop::markdown::link_target("https://example.com/docs", ""),
        Some("https://example.com/docs".into())
    );
    for target in [
        "src/main.rs",
        "/Users/nix/.env",
        "file:///etc/passwd",
        "javascript:alert(1)",
    ] {
        assert!(canopy_desktop::markdown::link_target(target, "").is_none());
    }
}
