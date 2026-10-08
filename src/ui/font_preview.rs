use super::components::media::*;
use super::{
    components::{column, files::file_message},
    theme as t,
};
use canopy_desktop::state::workspace::Pane;
use gpui_kit::*;
use std::sync::Arc;
pub struct FontPreview {
    image: Option<Arc<Image>>,
    family: String,
    details: String,
    error: Option<String>,
    task: Option<Task<()>>,
}
impl FontPreview {
    pub fn new(pane: Pane, cx: &mut Context<Self>) -> Self {
        let root = pane.metadata.cwd.unwrap_or_default();
        let path = std::path::PathBuf::from(pane.metadata.resource.unwrap_or_default());
        let color = serde_json::to_value(t::text())
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { canopy_desktop::font_preview::load(&root, &path, &color) })
                .await;
            let _ = this.update(cx, |this: &mut Self, cx| {
                match result {
                    Ok(font) => {
                        this.family = font.family;
                        this.details = format!(
                            "{} glyphs{}",
                            font.glyphs,
                            if font.missing {
                                " · Some sample characters are absent in this font subset"
                            } else {
                                ""
                            }
                        );
                        this.image = Some(Arc::new(Image::from_bytes(ImageFormat::Svg, font.svg)));
                    }
                    Err(e) => this.error = Some(e),
                }
                cx.notify();
            });
        });
        Self {
            image: None,
            family: "Loading font…".into(),
            details: String::new(),
            error: None,
            task: Some(task),
        }
    }
    pub fn release(&mut self, cx: &mut App) {
        self.task = None;
        if let Some(image) = self.image.take() {
            image.remove_asset(cx);
        }
    }
}
impl Render for FontPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        preview_surface()
            .p(px(24.))
            .gap(px(12.))
            .child(file_message(self.family.clone(), t::text()).text_size(px(18.)))
            .child(file_message(self.details.clone(), t::secondary()).text_size(px(11.)))
            .children(self.error.clone().map(|e| file_message(e, t::red())))
            .child(
                column()
                    .flex_1()
                    .min_h_0()
                    .children(self.image.clone().map(fitted_image)),
            )
    }
}
