//! Code blocks in the composer (DEC-088/089): ```` ``` ```` (with an optional language)
//! and ⇧Enter in a text field split it around a code block — a light monospace text
//! field with its language (DEC-090: no code editor, it was slow). The main field stays the
//! last one, under every block, so suggestions, badges, ↑ and the modes work as before;
//! each block has the text written above it in its own field. The message is the
//! markdown of all parts (`fences::join`), and one put back is split again.
use super::view::ChatView;
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::fences::{self, Part, Piece};
use gpui_kit::component::{
    input::{self, InputContent, InputEvent, Textarea, TextareaState},
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::*;
use std::ops::Range;

/// A block grows with its code up to this many lines, then scrolls.
const MAX_LINES: usize = 12;
const CODE_SIZE: f32 = 13.;

/// A code block and the text field above it.
pub(super) struct CodePart {
    pub before: Entity<TextareaState>,
    pub editor: Entity<TextareaState>,
    /// The fence's language tag (`rust`, `py`, empty for plain text).
    pub language: String,
    _events: [Subscription; 2],
}

/// Where a text field is: above block `n`, or the main field under them all.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Before(usize),
    Main,
}

/// The part of `content` in `range`, with the badges that lie inside it.
fn slice(content: &InputContent, range: Range<usize>) -> InputContent {
    let text = &content.text()[range.clone()];
    let mut part = InputContent::new(text.to_owned());
    for span in content.tokens() {
        let at = span.range();
        if at.start >= range.start && at.end <= range.end {
            let shifted = at.start - range.start..at.end - range.start;
            if let Ok(with) = part.clone().with_token(shifted, span.token().clone()) {
                part = with;
            }
        }
    }
    part
}

/// `first` then `second`, on a new line when both have text.
fn concat(first: &InputContent, second: &InputContent) -> (InputContent, usize) {
    let mut text = first.text().to_string();
    if !text.is_empty() && !second.text().is_empty() {
        text.push('\n');
    }
    let join = text.len();
    text.push_str(second.text());
    let mut content = InputContent::new(text);
    let spans = first
        .tokens()
        .iter()
        .map(|span| (span.range(), span.token()))
        .chain(second.tokens().iter().map(|span| {
            (
                span.range().start + join..span.range().end + join,
                span.token(),
            )
        }));
    for (range, token) in spans {
        if let Ok(with) = content.clone().with_token(range, token.clone()) {
            content = with;
        }
    }
    (content, join)
}

fn line_count(text: &str) -> usize {
    text.split('\n').count()
}

impl ChatView {
    /// The composer holds code blocks.
    pub(super) fn has_code_blocks(&self) -> bool {
        !self.code_parts.is_empty()
    }

    /// Nothing written in any part.
    pub(super) fn composer_empty(&self, cx: &App) -> bool {
        self.composer.read(cx).value().trim().is_empty()
            && self.code_parts.iter().all(|part| {
                part.before.read(cx).value().trim().is_empty()
                    && part.editor.read(cx).value().trim().is_empty()
            })
    }

    /// The message markdown of every part, in order.
    pub(super) fn composer_text(&self, cx: &App) -> String {
        if self.code_parts.is_empty() {
            return self.composer.read(cx).value().to_string();
        }
        let texts: Vec<(SharedString, SharedString)> = self
            .code_parts
            .iter()
            .map(|part| (part.before.read(cx).value(), part.editor.read(cx).value()))
            .collect();
        let main = self.composer.read(cx).value();
        let mut parts = vec![];
        for ((before, code), part) in texts.iter().zip(&self.code_parts) {
            parts.push(Part::Text(before));
            parts.push(Part::Code {
                language: &part.language,
                code,
            });
        }
        parts.push(Part::Text(&main));
        fences::join(parts)
    }

    /// Every text field's badges, in order (attachments go with the message by them).
    pub(super) fn composer_tokens(&self, cx: &App) -> Vec<SharedString> {
        self.code_parts
            .iter()
            .map(|part| &part.before)
            .chain([&self.composer])
            .flat_map(|field| {
                field
                    .read(cx)
                    .tokens()
                    .iter()
                    .map(|span| span.token().id().clone())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Puts a message into the composer: text with badges in the fields, fenced blocks
    /// as code blocks.
    pub(super) fn set_composer_content(
        &mut self,
        content: InputContent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.code_parts.clear();
        let pieces = fences::split(content.text());
        let mut pending: Option<InputContent> = None;
        for piece in pieces {
            match piece {
                Piece::Text(range) => pending = Some(slice(&content, range)),
                Piece::Code { language, code } => {
                    let before = pending.take().unwrap_or_else(|| InputContent::new(""));
                    let part = self.new_part(before, &language, &code, window, cx);
                    self.code_parts.push(part);
                }
            }
        }
        let main = pending.unwrap_or_else(|| InputContent::new(""));
        self.composer
            .update(cx, |state, cx| state.set_value(main, window, cx));
        cx.notify();
    }

    fn new_part(
        &mut self,
        before: InputContent,
        language: &str,
        code: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> CodePart {
        let field = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 8)
                .submit_on_enter(true)
        });
        field.update(cx, |state, cx| state.set_value(before, window, cx));
        let field_events = cx.subscribe_in(&field, window, |this, _, event, window, cx| {
            this.field_event(event, window, cx)
        });
        // Enter writes a new line here; the field grows with the code.
        let editor = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, MAX_LINES));
        editor.update(cx, |state, cx| state.set_value(code.to_owned(), window, cx));
        let editor_events = cx.subscribe_in(&editor, window, |_, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                // Send follows whether anything is written.
                cx.notify();
            }
        });
        CodePart {
            before: field,
            editor,
            language: language.to_owned(),
            _events: [field_events, editor_events],
        }
    }

    /// Enter in a field above a block does what it does in the main field.
    fn field_event(&mut self, event: &InputEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            InputEvent::PressEnter {
                secondary: true, ..
            } => self.command_enter(window, cx),
            InputEvent::PressEnter { shift: false, .. } => {
                if self.editing.is_some() {
                    self.send_edit(window, cx);
                } else if self.decision.active().is_some() {
                    self.decision_enter(window, cx);
                } else {
                    self.submit(window, cx);
                }
            }
            InputEvent::Change => cx.notify(),
            _ => {}
        }
    }

    fn field(&self, field: Field) -> &Entity<TextareaState> {
        match field {
            Field::Before(index) => &self.code_parts[index].before,
            Field::Main => &self.composer,
        }
    }

    /// The text field with focus.
    fn focused_field(&self, window: &Window, cx: &App) -> Option<Field> {
        if self.composer.focus_handle(cx).is_focused(window) {
            return Some(Field::Main);
        }
        self.code_parts
            .iter()
            .position(|part| part.before.focus_handle(cx).is_focused(window))
            .map(Field::Before)
    }

    /// The main field has focus (suggestions, ↑ and the modes act only there).
    pub(super) fn main_field_focused(&self, window: &Window, cx: &App) -> bool {
        self.composer.focus_handle(cx).is_focused(window)
    }

    /// The field under block `index`.
    fn field_after(&self, index: usize) -> Field {
        if index + 1 < self.code_parts.len() {
            Field::Before(index + 1)
        } else {
            Field::Main
        }
    }

    /// The block right above `field`.
    fn block_above(&self, field: Field) -> Option<usize> {
        match field {
            Field::Before(0) => None,
            Field::Before(index) => Some(index - 1),
            Field::Main => self.code_parts.len().checked_sub(1),
        }
    }

    /// ⇧Enter after a line of only ```` ``` ```` (and a language): the field splits
    /// around a new code block, which takes the cursor.
    pub(super) fn fence_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        // A waiting request's answer is plain text.
        if self.shell || self.goal_mode() || self.decision.active().is_some() {
            return false;
        }
        let Some(field) = self.focused_field(window, cx) else {
            return false;
        };
        let state = self.field(field).read(cx);
        let content = state.content();
        let cursor = state.cursor();
        let text = content.text().to_string();
        let line_start = text[..cursor].rfind('\n').map_or(0, |at| at + 1);
        let Some(language) = fences::opening(&text[line_start..cursor]).map(str::to_owned) else {
            return false;
        };
        let before = slice(&content, 0..line_start.saturating_sub(1));
        let after = slice(&content, cursor..text.len());
        let index = match field {
            Field::Before(index) => index,
            Field::Main => self.code_parts.len(),
        };
        let part = self.new_part(before, &language, "", window, cx);
        let editor = part.editor.clone();
        self.code_parts.insert(index, part);
        // What followed the cursor stays under the new block.
        let below = self.field(self.field_after(index)).clone();
        below.update(cx, |state, cx| state.set_value(after, window, cx));
        editor.update(cx, |state, cx| state.focus(window, cx));
        self.close_suggest(cx);
        cx.notify();
        true
    }

    /// ↑ at the top of a field goes into the block above it, at its end.
    pub(super) fn field_up(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(field) = self.focused_field(window, cx) else {
            return false;
        };
        let Some(block) = self.block_above(field) else {
            return false;
        };
        if self.field(field).read(cx).cursor_position().line != 0 {
            return false;
        }
        self.focus_block(block, true, window, cx);
        true
    }

    /// ↓ at the bottom of a field above a block goes into the block, at its start.
    pub(super) fn field_down(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(Field::Before(index)) = self.focused_field(window, cx) else {
            return false;
        };
        let state = self.field(Field::Before(index)).read(cx);
        if (state.cursor_position().line as usize) + 1 < line_count(&state.value()) {
            return false;
        }
        self.focus_block(index, false, window, cx);
        true
    }

    /// Backspace at the start of a field under a block goes to the block's end (an
    /// empty line between blocks then goes away).
    pub(super) fn field_backspace(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(field) = self.focused_field(window, cx) else {
            return false;
        };
        let Some(block) = self.block_above(field) else {
            return false;
        };
        if self.field(field).read(cx).selected_range() != (0..0) {
            return false;
        }
        self.focus_block(block, true, window, cx);
        true
    }

    fn focus_block(
        &mut self,
        index: usize,
        end: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = self.code_parts[index].editor.clone();
        editor.update(cx, |state, cx| {
            let at = if end { state.value().len() } else { 0 };
            state.set_selected_range(at..at, cx);
            state.focus(window, cx);
        });
    }

    fn focus_field(
        &mut self,
        field: Field,
        at: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = self.field(field).clone();
        field.update(cx, |state, cx| {
            state.set_selected_range(at..at, cx);
            state.focus(window, cx);
        });
    }

    /// ↑ on a block's first line: the field above it, at its end.
    fn block_up(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let editor = self.code_parts[index].editor.read(cx);
        if editor.cursor_position().line != 0 {
            return false;
        }
        let end = self.code_parts[index].before.read(cx).value().len();
        self.focus_field(Field::Before(index), end, window, cx);
        true
    }

    /// ↓ on a block's last line: the field under it, at its start.
    fn block_down(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let editor = self.code_parts[index].editor.read(cx);
        if (editor.cursor_position().line as usize) + 1 < line_count(&editor.value()) {
            return false;
        }
        self.focus_field(self.field_after(index), 0, window, cx);
        true
    }

    /// Backspace in an empty block removes it; the text above and under it join.
    fn block_backspace(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.code_parts[index].editor.read(cx).value().is_empty() {
            return false;
        }
        let below_field = self.field_after(index);
        let above = self.code_parts[index].before.read(cx).content();
        let below = self.field(below_field).read(cx).content();
        let (joined, at) = concat(&above, &below);
        let below_state = self.field(below_field).clone();
        self.code_parts.remove(index);
        below_state.update(cx, |state, cx| {
            state.set_value(joined, window, cx);
            state.set_selected_range(at..at, cx);
            state.focus(window, cx);
        });
        cx.notify();
        true
    }

    fn set_block_language(&mut self, index: usize, language: &str, cx: &mut Context<Self>) {
        let Some(part) = self.code_parts.get_mut(index) else {
            return;
        };
        part.language = language.to_owned();
        cx.notify();
    }

    /// The text fields and blocks above the main field, in order. An empty field above
    /// a block shows only while it has focus: blocks sit next to each other, and the
    /// arrows open a line between them that goes again when left empty.
    pub(super) fn code_part_elements(
        &mut self,
        field: impl Fn(&Entity<TextareaState>) -> AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut elements = vec![];
        for index in 0..self.code_parts.len() {
            let before = self.code_parts[index].before.clone();
            let shown =
                !before.read(cx).value().is_empty() || before.focus_handle(cx).is_focused(window);
            if shown {
                elements.push(field(&before));
            }
            elements.push(self.block(index, cx).into_any_element());
        }
        elements
    }

    fn block(&self, index: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let part = &self.code_parts[index];
        let editor = part.editor.clone();
        let name = fences::language_name(&part.language);
        let current = part.language.clone();
        let view = cx.entity().downgrade();
        let chip = super::composer::picker_button(
            ("chat-code-language", index),
            Some(custom_icon("code").size(px(12.)).into_any_element()),
            Some(name.clone()),
            format!("Language: {name}"),
            None,
            cx,
        )
        .dropdown_menu(move |mut menu, _, _| {
            menu = menu.label("Language");
            for (tag, label) in fences::LANGUAGES {
                let view = view.clone();
                let checked = fences::grammar(&current).unwrap_or("") == tag;
                menu = menu.item(PopupMenuItem::new(label).checked(checked).on_click(
                    move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| this.set_block_language(index, tag, cx));
                    },
                ));
            }
            menu.min_w(px(180.))
        });
        column()
            .id(("chat-code-block", index))
            .w_full()
            .rounded(px(8.))
            .border_1()
            .border_color(t::border())
            .bg(t::elevated())
            .overflow_hidden()
            // The block's own keys come before the field's: only ↑/↓ leave it (DEC-091),
            // Backspace removes it while empty, ⌘Enter sends instead of a new line.
            .capture_action(cx.listener(move |this, _: &input::MoveUp, window, cx| {
                if this.block_up(index, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(move |this, _: &input::MoveDown, window, cx| {
                if this.block_down(index, window, cx) {
                    cx.stop_propagation();
                }
            }))
            // Tab writes a tab, not the next control.
            .capture_action(
                cx.listener(move |this, _: &input::IndentInline, window, cx| {
                    cx.stop_propagation();
                    let editor = this.code_parts[index].editor.clone();
                    editor.update(cx, |state, cx| state.insert("\t", window, cx));
                }),
            )
            .capture_action(cx.listener(move |this, _: &input::Backspace, window, cx| {
                if this.block_backspace(index, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(move |this, enter: &input::Enter, window, cx| {
                if enter.secondary {
                    cx.stop_propagation();
                    this.command_enter(window, cx);
                }
            }))
            .child(
                row()
                    .h(px(30.))
                    .px(px(4.))
                    .border_b_1()
                    .border_color(t::border())
                    .child(chip),
            )
            .child(
                div().px(px(6.)).py(px(6.)).child(
                    Textarea::new(&editor)
                        .appearance(false)
                        .w_full()
                        .font_family(t::MONO)
                        .text_size(px(CODE_SIZE)),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{concat, slice};
    use gpui_kit::component::input::{InlineToken, InputContent};

    fn with_badge(text: &str, badge: &str) -> InputContent {
        let at = text.len() - badge.len();
        InputContent::new(text.to_owned())
            .with_token(at..text.len(), InlineToken::new("attachment-1", badge))
            .unwrap()
    }

    #[test]
    fn split_and_joined_fields_keep_their_badges() {
        let content = with_badge("look at [a.png]", "[a.png]");
        let tail = slice(&content, 5..content.text().len());
        assert_eq!(tail.text().as_ref(), "at [a.png]");
        assert_eq!(tail.tokens()[0].range(), 3..10);
        // A badge cut by the range is left out, not broken.
        assert!(slice(&content, 0..10).tokens().is_empty());
        let (joined, at) = concat(&InputContent::new("above"), &tail);
        assert_eq!((joined.text().as_ref(), at), ("above\nat [a.png]", 6));
        assert_eq!(joined.tokens()[0].range(), 9..16);
        let (joined, at) = concat(&InputContent::new(""), &tail);
        assert_eq!((joined.text().as_ref(), at), ("at [a.png]", 0));
    }
}
