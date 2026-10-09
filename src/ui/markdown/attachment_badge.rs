//! `canopy-attachment:` links in a sent message, drawn as the attachment's badge (the
//! chat supplies how a badge looks).
use super::BadgeRenderer;
use gpui_kit::{
    base::{
        InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext, MarkdownPlugin,
        markdown_ast::Node,
    },
    *,
};

const NAME: &str = "canopy-attachment";
pub(crate) const SCHEME: &str = "canopy-attachment:";

pub(super) struct AttachmentBadges {
    pub renderer: BadgeRenderer,
}

impl MarkdownPlugin for AttachmentBadges {
    fn name(&self) -> &str {
        NAME
    }

    fn parse(&self, node: &Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Link(link) = node else {
            return None;
        };
        let index: usize = link.url.strip_prefix(SCHEME)?.parse().ok()?;
        let label: String = link
            .children
            .iter()
            .map(|child| match child {
                Node::Text(text) => text.value.as_str(),
                _ => "",
            })
            .collect();
        Some(MarkdownNode::new(NAME, index).text(label))
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        let index: &usize = node.data()?;
        let height = (context.line_height() - px(2.)).max(context.font_size() + px(4.));
        Some(InlineElement::new((self.renderer)(
            *index, height, window, cx,
        )))
    }
}
