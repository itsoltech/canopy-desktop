//! Read-only image pane. File I/O and updates remain outside rendering.
use super::components::media::*;
use super::{components::files::file_message, theme as t};
use canopy_desktop::{
    files::{self, Watch},
    state::workspace::Pane,
};
use gpui_kit::*;
use std::{path::PathBuf, sync::Arc, time::Duration};
pub struct ImagePreview {
    image: Option<Arc<Image>>,
    error: Option<String>,
    loading: bool,
    root: PathBuf,
    path: PathBuf,
    read: Option<Task<()>>,
    watch: Option<Task<()>>,
}
fn format(path: &std::path::Path) -> Option<ImageFormat> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "gif" => Some(ImageFormat::Gif),
        "webp" => Some(ImageFormat::Webp),
        "svg" => Some(ImageFormat::Svg),
        "bmp" => Some(ImageFormat::Bmp),
        "ico" => Some(ImageFormat::Ico),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        _ => None,
    }
}
impl ImagePreview {
    pub fn new(pane: Pane, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            image: None,
            error: None,
            loading: true,
            root: pane.metadata.cwd.unwrap_or_default(),
            path: PathBuf::from(pane.metadata.resource.unwrap_or_default()),
            read: None,
            watch: None,
        };
        this.reload(cx);
        let parent = this
            .root
            .join(&this.path)
            .parent()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| this.root.clone());
        this.watch = Some(cx.spawn(async move |this, cx| {
            let watcher = cx
                .background_executor()
                .spawn(async move { Watch::new(&parent, false) })
                .await;
            let Ok(watcher) = watcher else {
                return;
            };
            let _ = this.update(cx, |this, cx| this.reload(cx));
            while watcher.changes.recv().await.is_ok() {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                while watcher.changes.try_recv().is_ok() {}
                if this.update(cx, |this, cx| this.reload(cx)).is_err() {
                    break;
                }
            }
        }));
        this
    }
    fn reload(&mut self, cx: &mut Context<Self>) {
        let (root, path) = (self.root.clone(), self.path.clone());
        self.read = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let format = format(&path).ok_or("Unsupported image format.")?;
                    files::image_bytes(&root, &path)
                        .map(|bytes| Arc::new(Image::from_bytes(format, bytes)))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(image) => {
                        if this
                            .image
                            .as_ref()
                            .is_some_and(|old| old.id() == image.id())
                        {
                            return;
                        }
                        if let Some(old) = this.image.replace(image) {
                            old.remove_asset(cx);
                        }
                        this.error = None;
                    }
                    Err(error) => {
                        if let Some(old) = this.image.take() {
                            old.remove_asset(cx);
                        }
                        this.error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
    }
    pub fn release(&mut self, cx: &mut App) {
        self.watch = None;
        self.read = None;
        if let Some(image) = self.image.take() {
            image.remove_asset(cx);
        }
    }
}
impl Render for ImagePreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        preview_surface()
            .p(px(16.))
            .items_center()
            .justify_center()
            .children(
                self.loading
                    .then(|| file_message("Loading image…", t::muted())),
            )
            .children(self.error.clone().map(|e| file_message(e, t::red())))
            .children(self.image.clone().map(|image| {
                fitted_image(image)
                    .with_loading(|| file_message("Decoding image…", t::muted()).into_any_element())
                    .with_fallback(|| {
                        file_message("This image could not be decoded.", t::red())
                            .into_any_element()
                    })
            }))
    }
}
