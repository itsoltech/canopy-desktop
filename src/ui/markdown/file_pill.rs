//! `canopy-file:` links in chat replies, drawn as pills that open the file in Canopy.
use crate::app_state::FilesState;
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::mentions::{self, Target};
use gpui_kit::{
    base::{
        InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext, MarkdownPlugin,
        markdown_ast::Node,
    },
    component::IconName,
    *,
};
use std::path::PathBuf;

const NAME: &str = "canopy-file";

/// Relative paths are inside `root`, the folder of the chat that wrote the reply.
pub(super) struct FilePills {
    pub root: PathBuf,
    /// With `part`, distinguishes element ids of pills in different replies and parts.
    pub owner: u64,
    pub part: usize,
}

impl MarkdownPlugin for FilePills {
    fn name(&self) -> &str {
        NAME
    }

    fn parse(&self, node: &Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Link(link) = node else {
            return None;
        };
        let target = mentions::parse_url(&link.url)?;
        let label: String = link
            .children
            .iter()
            .map(|child| match child {
                Node::Text(text) => text.value.as_str(),
                Node::InlineCode(code) => code.value.as_str(),
                _ => "",
            })
            .collect();
        Some(MarkdownNode::new(NAME, target).text(label))
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        _: &mut Window,
        _: &mut App,
    ) -> Option<InlineElement> {
        let target: &Target = node.data()?;
        let start = node.source_range().map_or(0, |range| range.start) as u64;
        let pill = pill(
            (
                "chat-file",
                super::element_key(self.owner, self.part, start as usize),
            ),
            &self.root,
            target,
            context.font_size() * 0.9,
            (context.line_height() - px(2.)).max(context.font_size() + px(4.)),
        );
        Some(InlineElement::new(pill))
    }
}

/// A file name with its line, opening the file in Canopy's editor when clicked.
/// `path` is relative to the chat folder `root`, or absolute; it is checked again on click.
pub(crate) fn pill(
    id: impl Into<ElementId>,
    root: &std::path::Path,
    target: &Target,
    size: Pixels,
    height: Pixels,
) -> Stateful<Div> {
    let name = target
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let label = match target.line {
        Some(line) => format!("{name}:{line}"),
        None => name,
    };
    let tooltip: SharedString = match target.line {
        Some(line) => format!("{}:{line}", target.path.display()),
        None => target.path.display().to_string(),
    }
    .into();
    let root = root.to_owned();
    let path = target.path.clone();
    let line = target.line;
    row()
        .id(id)
        .min_w_0()
        .max_w_full()
        .gap(px(4.))
        .px(px(6.))
        .h(height)
        .rounded(px(6.))
        // Close to the surface with a blue tint; the name reads like body text.
        .bg(t::accent().opacity(0.08))
        .border_1()
        .border_color(t::accent().opacity(0.14))
        .hover(|style| style.bg(t::accent().opacity(0.14)))
        .active(|style| style.bg(t::accent().opacity(0.2)))
        .cursor_pointer()
        .keyboard(format!("Open {}", target.path.display()))
        .text_size(size)
        .text_color(t::text())
        .child(match crate::ui::file_icons::for_path(&target.path) {
            Some(asset) => img(asset).size(size).flex_shrink_0().into_any_element(),
            None => icon(IconName::File)
                .size(size)
                .text_color(t::muted())
                .into_any_element(),
        })
        .child(div().min_w_0().truncate().child(label))
        .tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let root = root.clone();
            let mention = mentions::Mention {
                range: 0..0,
                label: String::new(),
                path: path.to_string_lossy().into_owned(),
                line,
            };
            cx.spawn(async move |cx| {
                let checked = cx
                    .background_executor()
                    .spawn(async move { mentions::resolve(&root, &mention) })
                    .await;
                if let Some(file) = checked {
                    cx.update(|cx| FilesState::open_in(file.root, &file.relative, file.line, cx));
                }
            })
            .detach();
        })
}
