//! Owned, selectable Markdown with background preparation and Canopy styles.
use super::{components::*, theme as t};
use gpui_kit::{
    base::{TextView, TextViewState, TextViewStyle},
    *,
};
use std::sync::Arc;

pub struct MarkdownView {
    text: Entity<TextViewState>,
    reading: bool,
    keep_previous: bool,
    source: Arc<str>,
    base_url: String,
    interactive: bool,
    loading: bool,
    revision: u64,
    prepare: Option<Task<()>>,
    _observer: Subscription,
}
impl MarkdownView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let text = cx.new(|cx| TextViewState::markdown("", cx));
        let observer = cx.observe(&text, |_, _, cx| cx.notify());
        Self {
            text,
            reading: false,
            keep_previous: false,
            source: Arc::from(""),
            base_url: String::new(),
            interactive: false,
            loading: false,
            revision: 0,
            prepare: None,
            _observer: observer,
        }
    }
    pub fn live(cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(cx);
        view.keep_previous = true;
        view
    }
    pub fn reading(cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(cx);
        view.reading = true;
        view
    }
    pub fn set_content(
        &mut self,
        source: Arc<str>,
        base_url: String,
        interactive: bool,
        cx: &mut Context<Self>,
    ) {
        if self.interactive != interactive {
            self.interactive = interactive;
            cx.notify();
        }
        if self.source == source && self.base_url == base_url {
            return;
        }
        let has_previous = !self.source.trim().is_empty() && !self.loading;
        self.source = source.clone();
        self.base_url = base_url.clone();
        self.revision = self.revision.wrapping_add(1);
        let revision = self.revision;
        self.prepare = None;
        if !self.keep_previous || source.trim().is_empty() {
            self.text.update(cx, |state, cx| state.set_text("", cx));
        }
        self.loading = !(source.trim().is_empty() || self.keep_previous && has_previous);
        if !source.trim().is_empty() {
            self.prepare = Some(cx.spawn(async move |this, cx| {
                let prepared = cx
                    .background_executor()
                    .spawn(async move { canopy_desktop::markdown::prepare(&source, &base_url) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.revision != revision {
                        return;
                    }
                    this.text
                        .update(cx, |state, cx| state.set_text(&prepared, cx));
                    this.loading = false;
                    cx.notify();
                });
            }));
        }
        cx.notify();
    }
}
fn markdown_style(reading: bool) -> TextViewStyle {
    let code = div()
        .id("markdown-code-style")
        .overflow_x_scroll()
        .p(px(8.))
        .rounded(px(4.))
        .font_family(t::MONO)
        .text_size(px(if reading { 13. } else { 12. }))
        .line_height(px(if reading { 20. } else { 18. }))
        .style()
        .clone();
    let table = div()
        .id("markdown-table-style")
        .overflow_x_scroll()
        .rounded(px(4.))
        .style()
        .clone();
    TextViewStyle::default()
        .with_dark(true)
        .with_foreground(t::text())
        .with_muted_foreground(t::secondary())
        .with_link(t::accent())
        .with_selection(t::accent().opacity(0.22))
        .with_border(t::control_border())
        .with_code_background(t::hover())
        .with_paragraph_gap(rems(0.65))
        .with_heading_font_size(move |level, _| {
            px(if reading {
                match level {
                    1 => 20.,
                    2 => 18.,
                    3 => 16.,
                    _ => 14.,
                }
            } else {
                match level {
                    1 => 16.,
                    2 => 15.,
                    3 => 14.,
                    _ => 13.,
                }
            })
        })
        .with_inline_code(HighlightStyle {
            color: Some(t::text()),
            background_color: Some(t::hover()),
            ..Default::default()
        })
        .with_code_block(code)
        .with_table(table)
        .with_table_cell(div().px(px(8.)).py(px(6.)).style().clone())
}
impl Render for MarkdownView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if self.loading {
            return column()
                .gap(px(8.))
                .child(skeleton_bar().w_full().h(px(10.)))
                .child(skeleton_bar().w(px(112.)).h(px(10.)))
                .into_any_element();
        }
        if self.source.trim().is_empty() {
            return div()
                .text_size(px(12.))
                .text_color(t::muted())
                .child("No description provided.")
                .into_any_element();
        }
        let base = self.base_url.clone();
        let interactive = self.interactive;
        TextView::new(&self.text)
            .style(markdown_style(self.reading))
            .w_full()
            .min_w_0()
            .text_size(px(if self.reading { 14. } else { 12. }))
            .line_height(px(if self.reading { 22. } else { 19. }))
            .scrollable(false)
            .selectable(interactive)
            .on_link_click(move |target, event, _, cx| {
                let activate = match event {
                    ClickEvent::Mouse(click) => {
                        matches!(click.up.button, MouseButton::Left | MouseButton::Middle)
                    }
                    ClickEvent::Keyboard(_) => true,
                    ClickEvent::Touch(click) => !click.long_press,
                };
                if interactive
                    && activate
                    && let Some(url) = canopy_desktop::markdown::link_target(target, &base)
                {
                    cx.open_url(&url);
                }
            })
            .into_any_element()
    }
}
