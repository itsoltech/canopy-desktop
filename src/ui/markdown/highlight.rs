//! Syntax highlighting for chat code blocks. Agents tag fences loosely (`Rust`, `TS`,
//! `jsx`, `zsh`), while the grammar registry matches exact lower-case names, so tags
//! are normalized before the bundled tree-sitter grammars are asked.
use gpui_kit::{
    base::text::CodeBlock,
    component::{
        ActiveTheme, Rope,
        highlighter::{HighlightTheme, SyntaxHighlighter},
    },
    *,
};
use std::{cell::RefCell, collections::HashMap, ops::Range, sync::Arc};

pub type Highlighter = dyn Fn(&CodeBlock) -> Vec<(Range<usize>, HighlightStyle)> + Send + Sync;

/// The grammar for a fence tag; `None` keeps the block plain.
pub fn language(tag: &str) -> Option<&'static str> {
    canopy_desktop::chat::fences::grammar(tag)
}

/// One highlighter per theme: a code block keeps its highlights while the `Arc` is
/// the same, so a fresh closure per frame would reparse every block every frame.
pub fn shared(cx: &App) -> Arc<Highlighter> {
    thread_local! {
        static SHARED: RefCell<Option<(Arc<HighlightTheme>, Arc<Highlighter>)>> =
            const { RefCell::new(None) };
    }
    let theme = cx.theme().highlight_theme.clone();
    SHARED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((cached, highlighter)) = cache.as_ref()
            && Arc::ptr_eq(cached, &theme)
        {
            return highlighter.clone();
        }
        let highlighter: Arc<Highlighter> = Arc::new(build(theme.clone()));
        *cache = Some((theme, highlighter.clone()));
        highlighter
    })
}

fn build(
    theme: Arc<HighlightTheme>,
) -> impl Fn(&CodeBlock) -> Vec<(Range<usize>, HighlightStyle)> + Send + Sync + 'static {
    move |block| {
        thread_local! {
            static PARSERS: RefCell<HashMap<&'static str, SyntaxHighlighter>> =
                RefCell::new(HashMap::new());
        }
        let Some(language) = block.lang().and_then(|tag| language(&tag)) else {
            return vec![];
        };
        let code = block.code();
        let rope = Rope::from_str(&code);
        PARSERS.with(|parsers| {
            let mut parsers = parsers.borrow_mut();
            let parser = parsers
                .entry(language)
                .or_insert_with(|| SyntaxHighlighter::new(language));
            parser.update(None, &rope, None);
            parser.styles(&(0..code.len()), theme.as_ref())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` brings GPUI's `test` attribute; these are plain tests.
    use core::prelude::v1::test;

    #[test]
    fn loose_fence_tags_map_to_bundled_grammars() {
        for (tag, expected) in [
            ("Rust", Some("rust")),
            ("rust,ignore", Some("rust")),
            ("TS", Some("typescript")),
            ("jsx", Some("javascript")),
            ("TSX", Some("typescript")),
            ("zsh", Some("bash")),
            ("console", Some("bash")),
            ("{.python}", Some("python")),
            ("YML", Some("yaml")),
            ("go", None),
            ("text", None),
            ("", None),
        ] {
            assert_eq!(language(tag), expected, "{tag}");
        }
    }

    /// Distinct colors: a plain block has one style run, a highlighted one several.
    fn colors(styles: &[(Range<usize>, HighlightStyle)]) -> usize {
        let mut colors: Vec<_> = styles.iter().filter_map(|(_, s)| s.color).collect();
        colors.dedup();
        colors.len()
    }

    #[test]
    fn capitalized_and_alias_tags_are_highlighted() {
        let theme = HighlightTheme::default_dark();
        let code = "fn main() { let x = 1; }";
        let rope = Rope::from_str(code);
        let mut exact = SyntaxHighlighter::new("Rust");
        exact.update(None, &rope, None);
        // The registry alone matches exact names: `Rust` stays plain without normalizing.
        assert!(colors(&exact.styles(&(0..code.len()), theme.as_ref())) <= 1);

        let highlight = build(theme);
        assert!(colors(&highlight(&CodeBlock::from_code(code, Some("Rust")))) > 1);
        let jsx = "const a = <b className=\"x\" />;";
        assert!(colors(&highlight(&CodeBlock::from_code(jsx, Some("jsx")))) > 1);
        assert!(highlight(&CodeBlock::from_code(code, Some("go"))).is_empty());
        assert!(highlight(&CodeBlock::from_code(code, None::<&str>)).is_empty());
    }
}
