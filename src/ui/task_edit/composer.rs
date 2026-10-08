use crate::ui::{components::*, markdown::MarkdownView, theme as t};
use gpui_kit::{
    base::Disableable,
    component::{
        input::{InputEvent, TextareaState},
        scroll::ScrollableElement,
    },
    *,
};
use std::sync::Arc;
pub struct ComposerChanged;
impl EventEmitter<ComposerChanged> for MarkdownComposer {}
pub struct MarkdownComposer {
    input: Entity<TextareaState>,
    preview: Entity<MarkdownView>,
    show_preview: bool,
    base: String,
    enabled: bool,
    height: f32,
    _events: Subscription,
}
impl MarkdownComposer {
    pub fn new(base: String, height: f32, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextareaState::new(window, cx).placeholder("Write in Markdown…"));
        let preview = cx.new(MarkdownView::new);
        let events = cx.subscribe(&input, |_, _, _: &InputEvent, cx| {
            cx.emit(ComposerChanged);
            cx.notify();
        });
        Self {
            input,
            preview,
            show_preview: false,
            base,
            enabled: true,
            height,
            _events: events,
        }
    }
    pub fn value(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }
    pub fn set_value(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.set_value(value.to_owned(), window, cx)
        });
        self.show_preview = false;
        cx.notify();
    }
    pub fn enable(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.enabled != enabled {
            self.enabled = enabled;
            if self.show_preview {
                let value = self.value(cx);
                self.preview.update(cx, |view, cx| {
                    view.set_content(Arc::from(value), self.base.clone(), enabled, cx)
                });
            }
            cx.notify();
        }
    }
}
impl Render for MarkdownComposer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs =
            row()
                .gap(px(4.))
                .children([(false, "Write"), (true, "Preview")].into_iter().map(
                    |(preview, label)| {
                        selection_button(label, label, self.show_preview == preview)
                            .border_0()
                            .disabled(!self.enabled)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.show_preview = preview;
                                if preview {
                                    let text = this.value(cx);
                                    this.preview.update(cx, |view, cx| {
                                        view.set_content(
                                            Arc::from(text),
                                            this.base.clone(),
                                            this.enabled,
                                            cx,
                                        )
                                    });
                                }
                                cx.notify();
                            }))
                    },
                ));
        column().gap(px(8.)).child(tabs).child(
            div()
                .h(px(self.height))
                .children((!self.show_preview).then(|| {
                    textarea(&self.input)
                        .w_full()
                        .h_full()
                        .disabled(!self.enabled)
                }))
                .children(self.show_preview.then(|| {
                    div()
                        .id("task-markdown-preview")
                        .size_full()
                        .overflow_y_scrollbar()
                        .p(px(8.))
                        .bg(t::hover())
                        .rounded(px(4.))
                        .child(self.preview.clone())
                })),
        )
    }
}
