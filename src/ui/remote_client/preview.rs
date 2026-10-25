//! A private, read-only preview owns its downloaded file for its full native lifetime.
use super::*;
use canopy_desktop::integrations::attachments::PreviewFile;

pub(super) fn open(
    file: Arc<PreviewFile>,
    media: String,
    cx: &mut App,
) -> Result<WindowHandle<Root>> {
    cx.open_window(
        WindowOptions {
            titlebar: Some(titlebar_options("Canopy — Remote attachment")),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(900.), px(680.)),
                cx,
            ))),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| {
                #[cfg(target_os = "macos")]
                let (sender, receiver) = async_channel::bounded(1);
                #[cfg(target_os = "macos")]
                let native = super::super::attachment_native::NativePreview::new(
                    file.clone(),
                    sender,
                    window,
                )
                .map(std::rc::Rc::new);
                #[cfg(target_os = "macos")]
                cx.spawn_in(window, async move |_, cx| {
                    if receiver.recv().await.is_ok() {
                        let _ = cx.update(|window, _| window.remove_window());
                    }
                })
                .detach();
                let focus = cx.focus_handle();
                focus.focus(window, cx);
                Preview {
                    file,
                    media,
                    focus,
                    saving: false,
                    font_image: None,
                    font_detail: None,
                    font_task: None,
                    #[cfg(target_os = "macos")]
                    error: native.as_ref().err().cloned(),
                    #[cfg(not(target_os = "macos"))]
                    error: None,
                    #[cfg(target_os = "macos")]
                    native: native.ok(),
                }
            });
            view.update(cx, |this, cx| {
                if this.media.starts_with("font/") {
                    let file = this.file.clone();
                    let color = serde_json::to_value(t::text())
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned();
                    this.font_task = Some(cx.spawn(async move |this, cx| {
                        let result = cx
                            .background_executor()
                            .spawn(async move {
                                let root =
                                    file.path().parent().ok_or("Font directory is missing.")?;
                                canopy_desktop::font_preview::load(root, file.path(), &color)
                            })
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            match result {
                                Ok(font) => {
                                    this.font_detail = Some(format!(
                                        "{} · {} glyphs{}",
                                        font.family,
                                        font.glyphs,
                                        if font.missing {
                                            " · sample contains missing glyphs"
                                        } else {
                                            ""
                                        }
                                    ));
                                    this.font_image = Some(Arc::new(Image::from_bytes(
                                        ImageFormat::Svg,
                                        font.svg,
                                    )));
                                }
                                Err(error) => this.error = Some(error),
                            }
                            cx.notify();
                        });
                    }));
                    cx.on_release(|this, cx| {
                        if let Some(image) = this.font_image.take() {
                            image.remove_asset(cx);
                        }
                    })
                    .detach();
                }
            });
            cx.new(|cx| Root::new(view, window, cx))
        },
    )
}
struct Preview {
    file: Arc<PreviewFile>,
    media: String,
    focus: FocusHandle,
    error: Option<String>,
    saving: bool,
    font_image: Option<Arc<Image>>,
    font_detail: Option<String>,
    font_task: Option<Task<()>>,
    #[cfg(target_os = "macos")]
    native: Option<std::rc::Rc<super::super::attachment_native::NativePreview>>,
}
impl Render for Preview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = column()
            .size_full()
            .bg(t::bg())
            .text_color(t::text())
            .track_focus(&self.focus)
            .on_key_down(|event, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    window.remove_window();
                }
            })
            .child(
                row()
                    .p_3()
                    .gap_2()
                    .child(
                        self.file
                            .path()
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                    )
                    .child(
                        button("save-attachment", "Save as…")
                            .disabled(self.saving)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let directory =
                                    match canopy_desktop::platform::directories::user_home() {
                                        Ok(path) => path,
                                        Err(error) => {
                                            this.error = Some(error);
                                            return;
                                        }
                                    };
                                let file = this.file.clone();
                                let picker = cx.prompt_for_new_path(
                                    &directory,
                                    file.path().file_name().and_then(|name| name.to_str()),
                                );
                                this.saving = true;
                                cx.spawn(async move |this, cx| {
                                    let result = async {
                                        let path = picker
                                            .await
                                            .map_err(|_| "Save dialog closed.".to_owned())?
                                            .map_err(|error| error.to_string())?;
                                        if let Some(path) = path {
                                            cx.background_executor()
                                                .spawn(async move { file.save_to(&path) })
                                                .await?;
                                        }
                                        Ok::<_, String>(())
                                    }
                                    .await;
                                    let _ = this.update(cx, |this, cx| {
                                        this.saving = false;
                                        this.error = result.err();
                                        cx.notify();
                                    });
                                })
                                .detach();
                            })),
                    ),
            );
        if let Some(error) = &self.error {
            body = body.child(error.clone());
        }
        if self.media.starts_with("font/") {
            return body.children(self.font_detail.clone()).children(
                self.font_image
                    .clone()
                    .map(super::super::components::media::fitted_image),
            );
        }
        #[cfg(target_os = "macos")]
        {
            let native = self.native.clone();
            body = body.child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, _, _| {
                        if let Some(native) = &native {
                            native.frame(bounds, 1.);
                        }
                    },
                )
                .size_full(),
            );
        }
        #[cfg(not(target_os = "macos"))]
        {
            if self.media.starts_with("image/") {
                body = body.child(super::super::components::media::fitted_image(
                    self.file.path().to_path_buf(),
                ));
            } else {
                body = body.child("Save this attachment to open it with a local application.");
            }
        }
        body
    }
}

// Signatures take precedence over names; never identify arbitrary HTML as an image.
pub(super) fn media_type(path: &str, bytes: &[u8]) -> &'static str {
    if let Ok(format) = image::guess_format(bytes) {
        return match format {
            image::ImageFormat::Png => "image/png",
            image::ImageFormat::Jpeg => "image/jpeg",
            image::ImageFormat::Gif => "image/gif",
            image::ImageFormat::WebP => "image/webp",
            image::ImageFormat::Bmp => "image/bmp",
            image::ImageFormat::Tiff => "image/tiff",
            image::ImageFormat::Ico => "image/x-icon",
            _ => "application/octet-stream",
        };
    }
    if bytes.starts_with(b"wOFF") {
        return "font/woff";
    }
    if bytes.starts_with(b"wOF2") {
        return "font/woff2";
    }
    if bytes.starts_with(b"OTTO") {
        return "font/otf";
    }
    if bytes.starts_with(&[0, 1, 0, 0]) {
        return "font/ttf";
    }
    if bytes.get(4..8) == Some(b"ftyp") {
        return "video/mp4";
    }
    if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        return "video/webm";
    }
    if std::str::from_utf8(bytes).is_ok() && !bytes.contains(&0) {
        return match std::path::Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "html" | "htm" => "text/html",
            "md" | "markdown" => "text/markdown",
            _ => "text/plain",
        };
    }
    "application/octet-stream"
}

pub(super) fn open_text(
    title: String,
    text: String,
    markdown: bool,
    cx: &mut App,
) -> Result<WindowHandle<Root>> {
    cx.open_window(
        WindowOptions {
            titlebar: Some(titlebar_options(&title)),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| {
                let rendered = markdown.then(|| cx.new(super::super::markdown::MarkdownView::chat));
                if let Some(view) = &rendered {
                    view.update(cx, |view, cx| {
                        view.set_content(Arc::from(text.as_str()), String::new(), true, cx)
                    });
                }
                let lines: Arc<Vec<String>> = Arc::new(text.lines().map(str::to_owned).collect());
                TextPreview {
                    text,
                    markdown: rendered,
                    list: ListState::new(lines.len(), ListAlignment::Top, px(300.)),
                    lines,
                }
            });
            cx.new(|cx| Root::new(view, window, cx))
        },
    )
}
struct TextPreview {
    text: String,
    markdown: Option<Entity<super::super::markdown::MarkdownView>>,
    lines: Arc<Vec<String>>,
    list: ListState,
}
impl Render for TextPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = column()
            .size_full()
            .bg(t::bg())
            .text_color(t::text())
            .gap_2()
            .child(
                button("copy-preview", "Copy").on_click(cx.listener(|this, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(this.text.clone()))
                })),
            );
        if let Some(markdown) = &self.markdown {
            body = body.child(
                div()
                    .id("preview-scroll")
                    .overflow_y_scroll()
                    .size_full()
                    .child(markdown.clone()),
            );
        } else {
            let lines = self.lines.clone();
            body = body.child(
                list(self.list.clone(), move |ix, _, _| {
                    let line = lines.get(ix).cloned().unwrap_or_default();
                    div()
                        .font_family(t::MONO)
                        .text_color(if line.starts_with('+') {
                            t::green()
                        } else if line.starts_with('-') {
                            t::red()
                        } else {
                            t::text()
                        })
                        .child(line)
                        .into_any_element()
                })
                .size_full(),
            );
        }
        body
    }
}

pub(super) fn open_html(html: String, cx: &mut App) -> Result<WindowHandle<Root>> {
    cx.open_window(
        WindowOptions {
            titlebar: Some(titlebar_options("Canopy — Page")),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| {
                // The inner document cannot navigate the native webview or call any app API.
                // It receives only already-authorized HTML bytes, no credentials or file URL.
                let wrapper = isolated_html(&html);
                let (page, error) =
                    match super::super::html_native::Page::new(&wrapper, "", 10000., window) {
                        Ok((page, events)) => {
                            page.set_active(true);
                            cx.spawn(async move |_, _| while events.recv().await.is_ok() {})
                                .detach();
                            (Some(std::rc::Rc::new(page)), None)
                        }
                        Err(error) => (None, Some(error)),
                    };
                HtmlPreview { page, error }
            });
            cx.new(|cx| Root::new(view, window, cx))
        },
    )
}
struct HtmlPreview {
    page: Option<std::rc::Rc<super::super::html_native::Page>>,
    error: Option<String>,
}
impl Render for HtmlPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let page = self.page.clone();
        column()
            .size_full()
            .bg(t::bg())
            .text_color(t::text())
            .children(self.error.clone())
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, _, _| {
                        if let Some(page) = &page {
                            page.place(bounds, bounds);
                        }
                    },
                )
                .size_full(),
            )
    }
}

fn isolated_html(html: &str) -> String {
    let policy = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; connect-src 'none'; form-action 'none'; frame-src 'none'\">";
    let source = format!("{policy}{html}")
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<!doctype html><style>html,body,iframe{{margin:0;width:100%;height:100%;border:0}}</style><iframe sandbox=\"allow-scripts\" srcdoc=\"{source}\"></iframe>"
    )
}

#[cfg(test)]
mod tests {
    use super::{isolated_html, media_type};
    #[test]
    fn html_cannot_escape_the_sandbox_attribute() {
        let page =
            isolated_html(r#""></iframe><script>window.top.location='file:///private'</script>"#);
        assert_eq!(page.matches("<iframe ").count(), 1);
        assert_eq!(page.matches("</iframe>").count(), 1);
        assert!(!page.contains("<script>"));
        assert!(page.contains("&lt;script&gt;"));
        assert!(page.contains("sandbox=\"allow-scripts\""));
    }
    #[test]
    fn workspace_media_uses_bytes_before_extension() {
        assert_eq!(
            media_type("image.txt", b"\x89PNG\r\n\x1a\nrest"),
            "image/png"
        );
        assert_eq!(media_type("image.png", b"<html>text</html>"), "text/plain");
        assert_eq!(media_type("note.md", b"# Note"), "text/markdown");
        assert_eq!(media_type("unknown", b"a\0b"), "application/octet-stream");
        assert_eq!(media_type("font", b"wOF2data"), "font/woff2");
    }
}
